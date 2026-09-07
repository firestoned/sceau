// Copyright (c) 2026 Erick Bourgeois, sceau
// SPDX-License-Identifier: Apache-2.0
//! Unit tests for `cli.rs` — the `serve`/`genesis`/`enroll`/`join`
//! subcommand surface. Pure argument parsing, no TPM/network required.

#[cfg(test)]
mod tests {
    use super::super::*;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        let mut full = vec!["sceau"];
        full.extend_from_slice(args);
        Cli::try_parse_from(full)
    }

    #[test]
    fn serve_defaults_to_standard_socket_path() {
        let cli = parse(&["serve"]).unwrap();
        assert_eq!(
            cli.command,
            Command::Serve {
                socket: "/run/sceau/sceau.sock".into(),
                force_legacy: false,
            }
        );
    }

    #[test]
    fn serve_honors_custom_socket() {
        let cli = parse(&["serve", "--socket", "/tmp/sceau.sock"]).unwrap();
        assert_eq!(
            cli.command,
            Command::Serve {
                socket: "/tmp/sceau.sock".into(),
                force_legacy: false,
            }
        );
    }

    #[test]
    fn serve_force_legacy_flag_is_honored() {
        let cli = parse(&["serve", "--force-legacy"]).unwrap();
        assert_eq!(
            cli.command,
            Command::Serve {
                socket: "/run/sceau/sceau.sock".into(),
                force_legacy: true,
            }
        );
    }

    #[test]
    fn genesis_defaults_to_not_forced() {
        let cli = parse(&["genesis"]).unwrap();
        assert_eq!(cli.command, Command::Genesis { force: false });
    }

    #[test]
    fn genesis_force_flag_is_honored() {
        let cli = parse(&["genesis", "--force"]).unwrap();
        assert_eq!(cli.command, Command::Genesis { force: true });
    }

    #[test]
    fn enroll_requires_listen_address() {
        assert!(parse(&["enroll"]).is_err());
    }

    #[test]
    fn enroll_with_listen_address_uses_defaults() {
        let cli = parse(&["enroll", "--listen", "0.0.0.0:8443"]).unwrap();
        assert_eq!(
            cli.command,
            Command::Enroll {
                listen: "0.0.0.0:8443".to_string(),
                max: DEFAULT_ENROLL_MAX,
                timeout_secs: DEFAULT_ENROLL_TIMEOUT_SECS,
                k0s_data_dir: certs::K0S_DEFAULT_DATA_DIR.into(),
            }
        );
    }

    #[test]
    fn enroll_honors_custom_max_and_timeout() {
        let cli = parse(&[
            "enroll",
            "--listen",
            "127.0.0.1:8443",
            "--max",
            "3",
            "--timeout-secs",
            "120",
        ])
        .unwrap();
        assert_eq!(
            cli.command,
            Command::Enroll {
                listen: "127.0.0.1:8443".to_string(),
                max: 3,
                timeout_secs: 120,
                k0s_data_dir: certs::K0S_DEFAULT_DATA_DIR.into(),
            }
        );
    }

    #[test]
    fn enroll_max_zero_is_rejected() {
        // Serving an enrollment listener that will never actually enroll
        // anyone is almost certainly a misconfiguration, not a valid
        // "serve nothing" mode — reject it rather than silently opening a
        // network listener that can never succeed.
        assert!(parse(&["enroll", "--listen", "127.0.0.1:8443", "--max", "0"]).is_err());
    }

    #[test]
    fn enroll_honors_custom_k0s_data_dir() {
        let cli = parse(&[
            "enroll",
            "--listen",
            "127.0.0.1:8443",
            "--k0s-data-dir",
            "/opt/k0s",
        ])
        .unwrap();
        assert_eq!(
            cli.command,
            Command::Enroll {
                listen: "127.0.0.1:8443".to_string(),
                max: DEFAULT_ENROLL_MAX,
                timeout_secs: DEFAULT_ENROLL_TIMEOUT_SECS,
                k0s_data_dir: "/opt/k0s".into(),
            }
        );
    }

    #[test]
    fn join_requires_seed() {
        assert!(parse(&["join"]).is_err());
    }

    #[test]
    fn join_with_seed_uses_defaults() {
        let cli = parse(&["join", "--seed", "node1.k8s.example.com:8443"]).unwrap();
        assert_eq!(
            cli.command,
            Command::Join {
                seed: "node1.k8s.example.com:8443".to_string(),
                k0s_data_dir: certs::K0S_DEFAULT_DATA_DIR.into(),
            }
        );
    }

    #[test]
    fn no_subcommand_is_rejected() {
        assert!(parse(&[]).is_err());
    }

    #[test]
    fn unknown_subcommand_is_rejected() {
        assert!(parse(&["bogus"]).is_err());
    }

    #[test]
    fn global_tcti_flag_works_before_subcommand() {
        let cli = parse(&["--tcti", "device:/dev/tpmrm1", "genesis"]).unwrap();
        assert_eq!(cli.tcti, "device:/dev/tpmrm1");
    }

    #[test]
    fn global_tcti_flag_works_after_subcommand() {
        let cli = parse(&["genesis", "--tcti", "device:/dev/tpmrm1"]).unwrap();
        assert_eq!(cli.tcti, "device:/dev/tpmrm1");
    }

    #[test]
    fn tcti_defaults_to_tpmrm0() {
        let cli = parse(&["serve"]).unwrap();
        assert_eq!(cli.tcti, "device:/dev/tpmrm0");
    }

    #[test]
    fn status_subcommand_takes_no_extra_args() {
        let cli = parse(&["status"]).unwrap();
        assert_eq!(cli.command, Command::Status);
    }

    #[test]
    fn status_rejects_unexpected_args() {
        assert!(parse(&["status", "--seed", "somewhere:8443"]).is_err());
    }
}
