//! Print what Settings → AI → Agent would detect on this machine:
//! `cargo run -p mystique-acp --example detect_agents`.

fn main() {
    let started = std::time::Instant::now();
    let agents = mystique_acp::detect_agents();
    for agent in &agents {
        let status = format!("{:?}", agent.status);
        let launch = match &agent.command {
            Some(command) => format!("{command} {}", agent.args.join(" ")),
            None => String::new(),
        };
        println!(
            "{:<20} {:<12} {:<28} {}",
            agent.name,
            status,
            agent.version.as_deref().unwrap_or("-"),
            launch.trim_end()
        );
        if agent.command.is_none() {
            if let Some(path) = &agent.path {
                println!("{:<20} found at {path}", "");
            }
        }
    }
    println!("{} agents checked in {:?}", agents.len(), started.elapsed());
}
