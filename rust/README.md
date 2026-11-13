# TeamHub (Rust)

Tasarım ve yazım sırası: [docs/TASARIM.md](docs/TASARIM.md). Şu an yalnızca iskelet var; canlı sistem hâlâ TypeScript sürümüdür.

## Windows'ta derleme

GCC ve MinGW araçları boşluklu yolları çalıştıramaz, bu yüzden kullanıcı klasörü adında boşluk varsa araçlar `C:\tc` altından kullanılır:

```bash
export RUSTUP_HOME='C:\tc\rustup' CARGO_HOME='C:\tc\cargo' CARGO_TARGET_DIR='C:\tc\target'
export PATH="/c/tc/mingw/bin:/c/tc/cargo/bin:$PATH"
```

Testler ve panel kontrolü:

```bash
cargo test --workspace --exclude teamhub-web
```

```bash
cargo check -p teamhub-web --target wasm32-unknown-unknown
```

Sunucu:

```bash
cargo run -p teamhub-server
```
