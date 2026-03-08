## Checklist

- [ ] I ran the Rust checks locally: `cargo fmt --all -- --check`, `cargo check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`
- [ ] I ran the frontend checks locally: `cd frontend && npm run lint && npm run build`
- [ ] If I changed build tooling, dependencies, or generated-asset behavior, I updated CI in `.github/workflows/ci.yml`
- [ ] If I added or changed setup requirements, I updated `README.md`
- [ ] If I added generated artifacts or build outputs, I updated `.gitignore`
- [ ] If I added new external tools, I documented how to install them
