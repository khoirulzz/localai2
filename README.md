# Nalaro AI Desktop

Native Windows x64 app built with Tauri 2, Rust, and a bundled llama.cpp llama-server runtime. GGUF models are imported by the user; no model weights are bundled.

The Windows installer is produced by GitHub Actions. Runtime llama.cpp binary is fetched from the pinned upstream release and integrity-checked during build.
