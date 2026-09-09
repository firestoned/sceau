// Copyright (c) 2026 Erick Bourgeois, sceau
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use super::super::*;

    #[test]
    fn node_name_from_cn_extracts_name() {
        assert_eq!(
            node_name_from_cn("system:node:node1.k8s.example.com").unwrap(),
            "node1.k8s.example.com"
        );
    }

    #[test]
    fn node_name_from_cn_rejects_non_node_identity() {
        assert!(node_name_from_cn("kube-apiserver-kubelet-client").is_err());
    }

    #[test]
    fn node_name_from_cn_rejects_bare_prefix_with_no_name() {
        assert!(node_name_from_cn("system:node:").is_err());
    }

    #[test]
    fn node_name_from_cn_rejects_empty_string() {
        assert!(node_name_from_cn("").is_err());
    }

    // ── authorize_allowlist ─────────────────────────────────────────────
    // The operator names the joiner at `enroll` time. Holding a valid k0s
    // node certificate is no longer sufficient on its own.

    fn allowed(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn authorize_allowlist_accepts_a_listed_node() {
        assert!(authorize_allowlist("k0s-node2", &allowed(&["k0s-node2"])).is_ok());
    }

    #[test]
    fn authorize_allowlist_accepts_any_listed_node_when_several_are_given() {
        let list = allowed(&["k0s-node2", "k0s-node3"]);
        assert!(authorize_allowlist("k0s-node3", &list).is_ok());
    }

    #[test]
    fn authorize_allowlist_rejects_an_unlisted_node() {
        // The F-1 case: a real, current cluster member that the operator
        // did not name — e.g. a worker whose kubelet cert is perfectly valid.
        assert!(authorize_allowlist("worker-7", &allowed(&["k0s-node2"])).is_err());
    }

    #[test]
    fn authorize_allowlist_fails_closed_on_an_empty_list() {
        // Belt and braces: the CLI requires at least one --allow-node, but if
        // an empty list ever reaches here it must deny, never allow-all.
        assert!(authorize_allowlist("k0s-node2", &[]).is_err());
    }

    #[test]
    fn authorize_allowlist_is_exact_not_prefix_or_substring() {
        let list = allowed(&["k0s-node2"]);
        assert!(authorize_allowlist("k0s-node2-evil", &list).is_err());
        assert!(authorize_allowlist("k0s-node", &list).is_err());
        assert!(authorize_allowlist("K0S-NODE2", &list).is_err());
    }

    #[test]
    fn authorize_allowlist_error_names_the_rejected_node() {
        let err = authorize_allowlist("worker-7", &allowed(&["k0s-node2"])).unwrap_err();
        assert!(format!("{err}").contains("worker-7"));
    }
}
