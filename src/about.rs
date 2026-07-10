use crate::config;

pub fn setup_about(about: libadwaita::AboutDialog) {
    setup_credits(&about);
    setup_issue_link(&about);
}

fn setup_credits(about: &libadwaita::AboutDialog) {
    let authors: Vec<&str> = include_str!("../AUTHORS")
        .trim_end_matches('\n')
        .split('\n')
        .collect();
    let translators = include_str!("../TRANSLATORS").trim_end_matches('\n');
    let artists: Vec<&str> = include_str!("../ARTISTS")
        .trim_end_matches('\n')
        .split('\n')
        .collect();
    about.set_version(config::VERSION);
    about.set_developers(&authors);
    about.set_translator_credits(translators);
    about.set_artists(&artists);
}

fn setup_issue_link(about: &libadwaita::AboutDialog) {
    about.connect_activate_link(|_, uri| {
        if uri.starts_with("https://github.com/Diegovsky/riff/issues") {
            let system_info = gather_system_info();
            let body = format!(
                "**Describe the bug**\n\
                 A clear and concise description of what the bug is.\n\n\
                 **To Reproduce**\n\
                 Steps to reproduce the behavior:\n\
                 1. Go to '...'\n\
                 2. Click on '....'\n\
                 3. Scroll down to '....'\n\
                 4. See error\n\n\
                 **Screenshots**\n\
                 If applicable, add screenshots to help explain your problem.\n\n\
                 **General information:**\n\
                 - Distribution: {distro}\n\
                 - Installation method: {install_method}\n\
                 - Riff Version: {version}\n\
                 - Arch: {arch}\n\
                 - Desktop: {desktop}\n\
                 - Display server: {display_server}\n\
                 - Device used: \n\n\
                 **Logs:**\n\
                 Please paste relevant log output below. You can retrieve logs with:\n\
                 ```sh\n\
                 journalctl --user _COMM=riff --since=\"1 hour ago\" --no-pager\n\
                 ```\n\
                 <details><summary>Log output</summary>\n\n\
                 ```\n\
                 (paste logs here)\n\
                 ```\n\n\
                 </details>\n\n\
                 **Additional context**\n\
                 Add any other context about the problem here.",
                distro = system_info.distro,
                install_method = system_info.install_method,
                version = system_info.version,
                arch = system_info.arch,
                desktop = system_info.desktop,
                display_server = system_info.display_server,
            );

            let url = format!(
                "https://github.com/Diegovsky/riff/issues/new?labels=bug&body={}",
                percent_encoding::utf8_percent_encode(&body, percent_encoding::NON_ALPHANUMERIC)
            );

            let _ = open::that(&url);
            true
        } else {
            false
        }
    });
}

struct SystemInfo {
    version: String,
    distro: String,
    arch: String,
    desktop: String,
    display_server: String,
    install_method: String,
}

fn gather_system_info() -> SystemInfo {
    let version = config::VERSION.to_string();
    let arch = std::env::consts::ARCH.to_string();

    let distro = std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|contents| {
            contents
                .lines()
                .find(|line| line.starts_with("PRETTY_NAME="))
                .map(|line| {
                    line.trim_start_matches("PRETTY_NAME=")
                        .trim_matches('"')
                        .to_string()
                })
        })
        .unwrap_or_else(|| format!("{} {}", std::env::consts::OS, arch));

    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    let display_server = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();
    let install_method = if std::path::Path::new("/.flatpak-info").exists() {
        "Flatpak".to_string()
    } else {
        "Native".to_string()
    };

    SystemInfo {
        version,
        distro,
        arch,
        desktop,
        display_server,
        install_method,
    }
}
