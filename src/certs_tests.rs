// Copyright (c) 2026 Erick Bourgeois, sceau
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use super::super::*;
    use rcgen::{CertificateParams, DnType, KeyPair};

    const TEST_NODE_NAME: &str = "node1.k8s.example.com";

    // Throwaway test-only key, generated via `openssl genrsa -traditional
    // 2048` -- never used for anything but this test, matches the PKCS#1
    // shape k0s actually writes for its CA key (see this module's doc
    // comment).
    const TEST_PKCS1_KEY_PEM: &str = "-----BEGIN RSA PRIVATE KEY-----
MIIEogIBAAKCAQEArV4ACvUKGskTmftJB3yBxNjYHdI1Y9r8heYFyutuyD6uL5cT
EmM16c42LgwQOKEqVIA5Di7a9MPuQdvRHdPvCdvaaTYpLjSrlqHupSq4z1bVqOSl
pqlCS534N93dtnvlTsSGDflCBJTc8WCG90QcgvovgM1Rkt/zlrEkt7o/jDiOVgIF
EEYq+cHXud6HZDEl52ZYsocC9AxKWrXN69QJdalZhqgHcB7YPfRrzPYmI1+rGbkZ
0giTwgUFTgrIUEvrkNYGmadhtfJe9uj9dw2+XFijPNcx7hzTHy7NevJd/DEuBAC3
rK2dLcfes5xANLLOf1OHtBFiU7TvwywN4Ob/IQIDAQABAoIBAA6PVnGVIsQdHwhP
klKOoldl3sCiZtl8Crk0GEhyfVtT6E0W4wMoUd7Q8rvrR3F6F5QBMMmAJeNokRn2
3BklM24giLdNVSgMRFziPKiibeL75/XHPiJBFNBE9BO9DmHFBf0XhCqoRGMeHXAo
Ky2ZCsv1NwgaNj+fj1WYivzjJ3MfKWI54d2QiGyejxqN9oGb3d6SAyDWBOywx+bm
Tkr6DcaV7TtqIX+M3Ip6FFcQegXUKRP193L+WKGkmcCJO92ZmBqa7Rwpj7ku5n92
UzTQgg2GIV+hRJHH9oTsnpdAss77FXW32tqgsEW18tPK+MMXSS0yJndeN/DOL9Zf
fMhv7gECgYEA35LChfeNotsUl2JTTXCp40AGG+W9qJJ1wscufAG9Ds/eI8ItvAVB
SN8+JlG50jT3hR3g2qXeURGdZL2AwnPAHtzrvfPkrHoqWtZpdEN2//1LEe2iedeD
ybLnDmYUPTHC4ImK028PkONhnDQcZVhVam+Woo71RDtfNBcv7DoDhLMCgYEAxoMY
m8YBLvMwbVA9CZlfGTz2e+xcbxel8aM9Z2+nsDMEVbSB4UrxFzb2U5FWUuTRISQT
QD6AC56sTncZEvBimqWie6qo2lCuuodX+E+x9YhYyBx6ei3UL4VvxNrarvH4g8RC
hCAOqEWlHlLzXS73nGlB7x7wPiP09YGZAgcKntsCgYAPFrGIJw/pCM4X9WvX0x20
F5MR+OxW3yORdK3fcqKWyFKeqTE6+kPQrjhcj7FxzV9THZQaTY12fTDZJqz08qjp
rFFAraAmP8xx+vx8+zyhxC9300je3juntio/34XIJ36WdtHmuR0c0yu4RhAQiuig
2U0aRXmqFDO1qUbzs2qfXwKBgF//k835EidvSZMDg5D5z4h398b0Bbtfl0tkotQ7
pb9K3KTJtymJQU/1r2e4WCOcLho1xO2DjA6SfEcxxzlmcHjS8uGVJTT2YZkozHzz
pV7UwgJ76yrcsMkOYX+0Sp7hu0mVhok4q33quDAS80ez5+CG8nC96HZUkyiKtMDL
QPKTAoGAKQycLOmjJffJfjnJkSy/ljf4sUYIIKl8AUqx/jTA5pa5I7XzlWNCYYfI
LriPYXIYsiZU6Ds2+8opb0bgZtxGX45kraWqTFOEKlSL4j4MdiL35wVhdBWCqUyl
/+J2JAHCEkvPqLitWffgX4RdN14tYj1VtUWuvH13i2nplAKkP/8=
-----END RSA PRIVATE KEY-----
";

    fn self_signed_test_cert(common_name: &str) -> rcgen::Certificate {
        let mut params = CertificateParams::new(vec![common_name.to_string()]).unwrap();
        params
            .distinguished_name
            .push(DnType::CommonName, common_name);
        let key = KeyPair::generate().unwrap();
        params.self_signed(&key).unwrap()
    }

    #[test]
    fn peer_cert_common_name_extracts_cn_from_der() {
        let cert = self_signed_test_cert(TEST_NODE_NAME);
        assert_eq!(peer_cert_common_name(cert.der()).unwrap(), TEST_NODE_NAME);
    }

    #[test]
    fn peer_cert_common_name_rejects_garbage_der() {
        assert!(peer_cert_common_name(&[0u8, 1, 2, 3]).is_err());
    }

    #[test]
    fn ca_key_to_pkcs8_pem_converts_pkcs1_rsa_key() {
        let pkcs8_pem = ca_key_to_pkcs8_pem(TEST_PKCS1_KEY_PEM).unwrap();
        assert!(pkcs8_pem.contains("BEGIN PRIVATE KEY"));
        // rcgen's `ring` backend (what actually failed against k0s's real
        // CA key) must be able to parse the result -- this is the real
        // assertion.
        KeyPair::from_pem(&pkcs8_pem).unwrap();
    }

    #[test]
    fn ca_key_to_pkcs8_pem_passes_through_pkcs8_key_unchanged() {
        let key = KeyPair::generate().unwrap();
        let pkcs8_pem = key.serialize_pem();
        assert_eq!(ca_key_to_pkcs8_pem(&pkcs8_pem).unwrap(), pkcs8_pem);
    }
}
