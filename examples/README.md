# SDK usage examples

- `golden_devnet.rs`  
  Full register -> upload -> download -> hash-check flow.

- `from_env_settings.rs`  
  Shows how an app can load SDK config from environment and create `CascadeSdk`.

- `custom_config.rs`  
  Shows explicit in-code endpoint config for app integration.

Run:

```bash
cargo run --example custom_config
cargo run --example from_env_settings
cargo run --example golden_devnet
```
