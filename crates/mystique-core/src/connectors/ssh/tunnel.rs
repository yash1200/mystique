use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use mystique_connector_api::{ConnectorError, ConnectorResult, SshHostKeyPrompt};
use russh::client::{self, Handler};
use russh::keys::key::PrivateKeyWithHashAlg;
use russh::keys::ssh_key::{HashAlg, PublicKey};
use russh::keys::{load_secret_key, PublicKeyOrCertificate};
use russh::{client::AuthResult, ChannelMsg, Disconnect};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{watch, Mutex};
use tokio::task::JoinSet;

/// A blackholed bastion must fail here, not at the OS TCP timeout (~75s).
pub(crate) const SSH_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const SSH_KEEPALIVE: Duration = Duration::from_secs(30);

use crate::domain::{SshAuth, SshTunnelRequest};

struct HostKeyHandler {
    trusted: Option<String>,
    presented: Arc<Mutex<Option<SshHostKeyPrompt>>>,
    host: String,
    port: u16,
}

impl Handler for HostKeyHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let (fingerprint, key_type, public_key) = describe_key(server_public_key);
        let prompt = if self.trusted.as_deref().is_some_and(|t| t == fingerprint) {
            None
        } else if self.trusted.is_some() {
            Some(SshHostKeyPrompt::mismatch(
                self.host.clone(),
                self.port,
                fingerprint,
                key_type,
                public_key,
            ))
        } else {
            Some(SshHostKeyPrompt::unknown(
                self.host.clone(),
                self.port,
                fingerprint,
                key_type,
                public_key,
            ))
        };
        if let Some(p) = prompt {
            *self.presented.lock().await = Some(p);
            Ok(false)
        } else {
            Ok(true)
        }
    }
}

fn describe_key(key: &PublicKeyOrCertificate) -> (String, String, String) {
    match key {
        PublicKeyOrCertificate::PublicKey { key, .. } => describe_public(key),
        PublicKeyOrCertificate::Certificate(cert) => describe_public(&PublicKey::new(
            cert.public_key().clone(),
            cert.comment().to_string(),
        )),
    }
}

fn describe_public(key: &PublicKey) -> (String, String, String) {
    let fingerprint = key.fingerprint(HashAlg::Sha256).to_string();
    let key_type = key.algorithm().to_string();
    let public_key = key.to_openssh().unwrap_or_default();
    (fingerprint, key_type, public_key)
}

/// Shared after authentication.
///
/// `channel_open_direct_tcpip` and `disconnect` only send on the session
/// channel. They do not read the reply receiver, so concurrent forwards do
/// not take a mutex across the open.
struct SharedSession(client::Handle<HostKeyHandler>);

// SAFETY: see the struct comment. No method used after `authenticate`
// touches the reply receiver.
unsafe impl Sync for SharedSession {}

struct ForwardSet {
    tasks: Mutex<JoinSet<()>>,
}

impl ForwardSet {
    fn new() -> Self {
        Self {
            tasks: Mutex::new(JoinSet::new()),
        }
    }

    async fn spawn(&self, fut: impl std::future::Future<Output = ()> + Send + 'static) {
        self.tasks.lock().await.spawn(fut);
    }

    async fn shutdown(&self) {
        let mut tasks = self.tasks.lock().await;
        tasks.abort_all();
        while tasks.join_next().await.is_some() {}
    }
}

/// Local TCP forward kept alive for a session.
pub struct SshTunnel {
    pub local_port: u16,
    shutdown: watch::Sender<bool>,
    accept: tokio::task::JoinHandle<()>,
    forwards: Arc<ForwardSet>,
    session: Option<Arc<SharedSession>>,
}

impl SshTunnel {
    /// Abort local forwards and send SSH disconnect.
    pub async fn close(mut self) {
        let _ = self.shutdown.send(true);
        self.accept.abort();
        self.forwards.shutdown().await;
        if let Some(session) = self.session.take() {
            let _ = session
                .0
                .disconnect(Disconnect::ByApplication, "Mystique session closed", "")
                .await;
        }
    }
}

impl Drop for SshTunnel {
    fn drop(&mut self) {
        let _ = self.shutdown.send(true);
        self.accept.abort();
        if let Ok(mut tasks) = self.forwards.tasks.try_lock() {
            tasks.abort_all();
        }
        // `close` already sent disconnect. This is a tunnel dropped without
        // `close` (failed open, process teardown).
        let Some(session) = self.session.take() else {
            return;
        };
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let _ = session
                    .0
                    .disconnect(Disconnect::ByApplication, "Mystique session closed", "")
                    .await;
            });
        }
    }
}

pub async fn open_tunnel(
    req: &SshTunnelRequest,
    dest_host: &str,
    dest_port: u16,
) -> ConnectorResult<SshTunnel> {
    if req.user.trim().is_empty() {
        return Err(ConnectorError::InvalidConfig(
            "SSH user is required.".into(),
        ));
    }
    let presented = Arc::new(Mutex::new(None));
    let handler = HostKeyHandler {
        trusted: req.trusted_fingerprint.clone(),
        presented: Arc::clone(&presented),
        host: req.host.clone(),
        port: req.port,
    };
    let config = Arc::new(client::Config {
        nodelay: true,
        keepalive_interval: Some(SSH_KEEPALIVE),
        keepalive_max: 3,
        ..Default::default()
    });
    let connect = client::connect(config, (req.host.as_str(), req.port), handler);
    let connected = match wait_connect(connect).await {
        Ok(inner) => inner,
        Err(timeout) => return Err(timeout),
    };
    let mut session = match connected {
        Ok(session) => session,
        Err(err) => return Err(host_key_or_connect(err, &presented).await),
    };

    authenticate(&mut session, req).await?;

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| ConnectorError::Connect(format!("SSH local listen failed: {e}")))?;
    let local_port = listener
        .local_addr()
        .map_err(|e| ConnectorError::Connect(e.to_string()))?
        .port();

    let session = Arc::new(SharedSession(session));
    let forwards = Arc::new(ForwardSet::new());
    let (shutdown, rx) = watch::channel(false);
    let dest_host = dest_host.to_string();
    let accept = tokio::spawn(accept_loop(
        listener,
        Arc::clone(&session),
        Arc::clone(&forwards),
        dest_host,
        dest_port,
        rx,
    ));

    Ok(SshTunnel {
        local_port,
        shutdown,
        accept,
        forwards,
        session: Some(session),
    })
}

pub(crate) async fn wait_connect<T>(
    connect: impl std::future::Future<Output = T>,
) -> ConnectorResult<T> {
    match tokio::time::timeout(SSH_CONNECT_TIMEOUT, connect).await {
        Ok(value) => Ok(value),
        Err(_) => Err(ConnectorError::Connect(
            "SSH connection timed out after 10s.".into(),
        )),
    }
}

async fn host_key_or_connect(
    err: russh::Error,
    presented: &Arc<Mutex<Option<SshHostKeyPrompt>>>,
) -> ConnectorError {
    if let Some(prompt) = presented.lock().await.take() {
        return ConnectorError::SshHostKey(prompt);
    }
    ConnectorError::Connect(format!("SSH: {err}"))
}

async fn authenticate(
    session: &mut client::Handle<HostKeyHandler>,
    req: &SshTunnelRequest,
) -> ConnectorResult<()> {
    let user = req.user.trim();
    let result = match req.auth {
        SshAuth::Password => {
            let pw = req
                .password
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| ConnectorError::Auth("SSH password is required.".into()))?;
            session
                .authenticate_password(user, pw)
                .await
                .map_err(|e| ConnectorError::Auth(format!("SSH password auth: {e}")))?
        }
        SshAuth::Key => authenticate_key(session, req).await?,
        SshAuth::Agent => authenticate_agent(session, user).await?,
    };
    if !result.success() {
        return Err(ConnectorError::Auth("SSH authentication failed.".into()));
    }
    Ok(())
}

async fn authenticate_key(
    session: &mut client::Handle<HostKeyHandler>,
    req: &SshTunnelRequest,
) -> ConnectorResult<AuthResult> {
    let path = req
        .private_key_path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ConnectorError::InvalidConfig("SSH private key path is required.".into()))?;
    let passphrase = req
        .passphrase
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let key = load_secret_key(path, passphrase)
        .map_err(|e| ConnectorError::Auth(format!("SSH key file: {e}")))?;
    let hash = session
        .best_supported_rsa_hash()
        .await
        .map_err(|e| ConnectorError::Auth(e.to_string()))?
        .flatten();
    session
        .authenticate_publickey(
            req.user.trim(),
            PrivateKeyWithHashAlg::new(Arc::new(key), hash),
        )
        .await
        .map_err(|e| ConnectorError::Auth(format!("SSH public-key auth: {e}")))
}

async fn authenticate_agent(
    session: &mut client::Handle<HostKeyHandler>,
    user: &str,
) -> ConnectorResult<AuthResult> {
    #[cfg(unix)]
    {
        use russh::keys::agent::client::AgentClient;
        use russh::keys::agent::AgentIdentity;
        let mut agent = AgentClient::connect_env().await.map_err(|e| {
            ConnectorError::Auth(format!(
                "SSH agent unavailable ({e}). Use a password or key file."
            ))
        })?;
        let ids = agent
            .request_identities()
            .await
            .map_err(|e| ConnectorError::Auth(format!("SSH agent identities: {e}")))?;
        if ids.is_empty() {
            return Err(ConnectorError::Auth("SSH agent has no keys.".into()));
        }
        let hash = session
            .best_supported_rsa_hash()
            .await
            .map_err(|e| ConnectorError::Auth(e.to_string()))?
            .flatten();
        for id in ids {
            let key = match id {
                AgentIdentity::PublicKey { key, .. } => key,
                AgentIdentity::Certificate { .. } => continue,
            };
            match session
                .authenticate_publickey_with(user, key, hash, &mut agent)
                .await
            {
                Ok(res) if res.success() => return Ok(res),
                Ok(_) => {}
                Err(e) => {
                    return Err(ConnectorError::Auth(format!("SSH agent: {e}")));
                }
            }
        }
        Err(ConnectorError::Auth("SSH agent keys were rejected.".into()))
    }
    #[cfg(windows)]
    {
        use russh::keys::agent::client::AgentClient;
        use russh::keys::agent::AgentIdentity;
        let mut agent = AgentClient::connect_pageant().await.map_err(|e| {
            ConnectorError::Auth(format!(
                "SSH agent (Pageant) unavailable ({e}). Use a password or key file."
            ))
        })?;
        let ids = agent
            .request_identities()
            .await
            .map_err(|e| ConnectorError::Auth(format!("SSH agent identities: {e}")))?;
        if ids.is_empty() {
            return Err(ConnectorError::Auth("SSH agent has no keys.".into()));
        }
        let hash = session
            .best_supported_rsa_hash()
            .await
            .map_err(|e| ConnectorError::Auth(e.to_string()))?
            .flatten();
        for id in ids {
            let key = match id {
                AgentIdentity::PublicKey { key, .. } => key,
                AgentIdentity::Certificate { .. } => continue,
            };
            match session
                .authenticate_publickey_with(user, key, hash, &mut agent)
                .await
            {
                Ok(res) if res.success() => return Ok(res),
                Ok(_) => {}
                Err(e) => {
                    return Err(ConnectorError::Auth(format!("SSH agent: {e}")));
                }
            }
        }
        Err(ConnectorError::Auth("SSH agent keys were rejected.".into()))
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (session, user);
        Err(ConnectorError::NotSupported(
            "SSH agent is not available on this platform.".into(),
        ))
    }
}

async fn accept_loop(
    listener: TcpListener,
    handle: Arc<SharedSession>,
    forwards: Arc<ForwardSet>,
    dest_host: String,
    dest_port: u16,
    mut shutdown: watch::Receiver<bool>,
) {
    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    break;
                }
            }
            accepted = listener.accept() => {
                let Ok((stream, origin)) = accepted else { break };
                let handle = Arc::clone(&handle);
                let dest_host = dest_host.clone();
                forwards
                    .spawn(async move {
                        let _ = forward_one(handle, stream, origin, dest_host, dest_port).await;
                    })
                    .await;
            }
        }
    }
}

async fn forward_one(
    handle: Arc<SharedSession>,
    mut stream: TcpStream,
    origin: SocketAddr,
    dest_host: String,
    dest_port: u16,
) -> ConnectorResult<()> {
    // No lock across this await: each forward opens its own channel.
    let mut channel = handle
        .0
        .channel_open_direct_tcpip(
            dest_host,
            u32::from(dest_port),
            origin.ip().to_string(),
            u32::from(origin.port()),
        )
        .await
        .map_err(|e| ConnectorError::Connect(format!("SSH forward: {e}")))?;
    let mut stream_closed = false;
    let mut buf = vec![0u8; 32 * 1024];
    loop {
        tokio::select! {
            r = stream.read(&mut buf), if !stream_closed => {
                match r {
                    Ok(0) => {
                        stream_closed = true;
                        let _ = channel.eof().await;
                    }
                    Ok(n) => {
                        channel
                            .data(&buf[..n])
                            .await
                            .map_err(|e| ConnectorError::Connect(e.to_string()))?;
                    }
                    Err(_) => break,
                }
            }
            msg = channel.wait() => {
                match msg {
                    Some(ChannelMsg::Data { ref data }) => {
                        if stream.write_all(data).await.is_err() {
                            break;
                        }
                    }
                    Some(ChannelMsg::Eof) | None => break,
                    Some(ChannelMsg::WindowAdjusted { .. }) => {}
                    Some(_) => {}
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn blackholed_bastion_times_out_in_about_ten_seconds() {
        let req = crate::domain::SshTunnelRequest {
            host: "192.0.2.1".into(),
            port: 22,
            user: "tunnel".into(),
            auth: crate::domain::SshAuth::Password,
            password: Some("unused".into()),
            private_key_path: None,
            passphrase: None,
            trusted_fingerprint: None,
        };
        let started = std::time::Instant::now();
        let err = match open_tunnel(&req, "db.internal", 5432).await {
            Err(err) => err,
            Ok(_) => panic!("blackholed bastion connected"),
        };
        let elapsed = started.elapsed();
        assert!(
            elapsed >= Duration::from_secs(9) && elapsed < Duration::from_secs(15),
            "{elapsed:?} {err}"
        );
        assert!(err.to_string().contains("timed out after 10s"), "{err}");
    }

    #[tokio::test]
    async fn close_aborts_forward_tasks() {
        let forwards = ForwardSet::new();
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        forwards
            .spawn(async move {
                let _ = rx.await;
            })
            .await;
        forwards.shutdown().await;
        assert!(tx.is_closed());
    }
}
