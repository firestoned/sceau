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
}
