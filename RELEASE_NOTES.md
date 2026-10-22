# Release Notes

## Security

- **Distrust compromised emSign certificate chains.** Update the bundled CA
  certificates (`webpki-root-certs` 1.0.9 → 1.0.10), removing
  `emSign ECC Root CA - G3`. This responds to the disclosure of intermediate
  CA private keys belonging to India's eMudhra/emSign and prevents ClewdR's
  bundled trust store from accepting chains through those intermediates.
  The update protects upstream HTTPS connections against public-network
  man-in-the-middle tools using the disclosed keys to forge certificates.
- **Upgrade recommended.** Install v0.13.6 and restart ClewdR; container users
  should pull the updated image and recreate their containers. Updating the
  operating system's CA bundle alone does not update the roots embedded in
  existing ClewdR binaries.

References: [RUSTSEC-2026-0383 / CVE-2026-111943](https://rustsec.org/advisories/RUSTSEC-2026-0383.html),
[upstream root removal](https://github.com/rustls/webpki-roots/pull/133).

## 安全更新

- 因印度 CA 机构 eMudhra/emSign 的中间 CA 私钥泄露，将内置 CA 包
  `webpki-root-certs` 从 1.0.9 更新至 1.0.10，移除
  `emSign ECC Root CA - G3` 信任根，防范公网中间人工具利用泄露密钥
  伪造证书、拦截 ClewdR 的上游 HTTPS 通信。
- 建议升级到 v0.13.6 并重启；容器用户请拉取新镜像并重建容器。
  仅更新操作系统 CA 包不会更新已有 ClewdR 二进制内嵌的信任根。
