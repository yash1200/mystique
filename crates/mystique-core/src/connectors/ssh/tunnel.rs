use std::net::SocketAddr;
use std::sync::Arc;

use mystique_connector_api::{ConnectorError, ConnectorResult, SshHostKeyPrompt};
use russh::client::{self, Handler};
use russh::keys::key::PrivateKeyWithHashAlg;
use russh::keys::ssh_key::{HashAlg, PublicKey};
use russh::keys::{load_secret_key, PublicKeyOrCertificate};
use russh::{client::AuthResult, ChannelMsg};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{watch, Mutex};

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

/// Local TCP forward kept alive for a session.
pub struct SshTunnel {
    pub local_port: u16,
    shutdown: watch::Sender<bool>,
    join: tokio::task::JoinHandle<()>,
    _session: Arc<Mutex<client::Handle<HostKeyHandler>>>,
}

impl Drop for SshTunnel {
    fn drop(&mut self) {
        let _ = self.shutdown.send(true);
        self.join.abort();
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
        ..Default::default()
    });
    let mut session = match client::connect(config, (req.host.as_str(), req.port), handler).await {
        Ok(s) => s,
        Err(e) => return Err(host_key_or_connect(e, &presented).await),
    };

    authenticate(&mut session, req).await?;

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| ConnectorError::Connect(format!("SSH local listen failed: {e}")))?;
    let local_port = listener
        .local_addr()
        .map_err(|e| ConnectorError::Connect(e.to_string()))?
        .port();

    let handle = Arc::new(Mutex::new(session));
    let (shutdown, rx) = watch::channel(false);
    let dest_host = dest_host.to_string();
    let join = tokio::spawn(accept_loop(
        listener,
        Arc::clone(&handle),
        dest_host,
        dest_port,
        rx,
    ));

    Ok(SshTunnel {
        local_port,
        shutdown,
        join,
        _session: handle,
    })
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
    handle: Arc<Mutex<client::Handle<HostKeyHandler>>>,
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
                tokio::spawn(async move {
                    let _ = forward_one(handle, stream, origin, dest_host, dest_port).await;
                });
            }
        }
    }
}

async fn forward_one(
    handle: Arc<Mutex<client::Handle<HostKeyHandler>>>,
    mut stream: TcpStream,
    origin: SocketAddr,
    dest_host: String,
    dest_port: u16,
) -> ConnectorResult<()> {
    let mut channel = {
        let h = handle.lock().await;
        h.channel_open_direct_tcpip(
            dest_host,
            u32::from(dest_port),
            origin.ip().to_string(),
            u32::from(origin.port()),
        )
        .await
        .map_err(|e| ConnectorError::Connect(format!("SSH forward: {e}")))?
    };
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
