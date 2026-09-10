# rust/environment-variable-access

Reject configured ambient process inputs outside explicit composition or platform boundaries. Capture environment and host paths once, validate them into owned configuration, and pass that configuration to consumers. Optionally reject configuration/state globals backed by known lazy initialization types.

```toml
[[rules."rust/environment-variable-access"]]
target = "**/*.rs"
exclude = "**/fixtures/**"
scope = "production"
allowed_targets = ["apps/**/*.rs", "**/build.rs"]
allowed_modules = ["platform", "adapter::environment"]
functions = [
    "std::env::var", "std::env::var_os", "std::env::vars", "std::env::vars_os",
    "std::env::set_var", "std::env::remove_var",
    "std::env::current_dir", "std::env::current_exe", "std::env::temp_dir",
    "dirs::home_dir", "dirs::audio_dir", "dirs::cache_dir", "dirs::config_dir",
    "dirs::config_local_dir", "dirs::data_dir", "dirs::data_local_dir",
    "dirs::desktop_dir", "dirs::document_dir", "dirs::download_dir",
    "dirs::executable_dir", "dirs::font_dir", "dirs::picture_dir",
    "dirs::preference_dir", "dirs::public_dir", "dirs::runtime_dir",
    "dirs::state_dir", "dirs::template_dir", "dirs::video_dir",
]
global_types = [
    "std::sync::OnceLock", "std::sync::LazyLock",
    "once_cell::sync::OnceCell", "once_cell::sync::Lazy",
    "once_cell::unsync::OnceCell", "once_cell::unsync::Lazy",
]
global_words = ["config", "configuration", "settings", "state"]
```

`functions` is required and nonempty; entries are exact API paths. There is no hidden API list or built-in permission for files named `main.rs`, `build.rs`, `host.rs`, or `platform.rs`. `target`, `exclude`, and `allowed_targets` accept a glob or list. `allowed_modules` contains exact crate-relative Rust namespace prefixes: `platform` permits `platform::unix` but not `platform_extra`. Filesystem and inline module identities use the shared Rust declaration analysis. `scope` accepts `production` (default), `tests`, or `all`.

```rust
use std::env::var as read_environment;
fn load() {
    read_environment("SERVICE_URL"); // Error outside a configured boundary.
}
```

Calls resolve through explicit imports, aliases, grouped imports, and qualified paths. A local function or binding named `var`, an unrelated `my_dirs::home_dir`, comments, and strings are not evidence. Unknown calls remain unclassified. Shared AST analysis does not expand macros, resolve arbitrary cross-file reexports, or infer calls through function pointers.

Both `global_types` and `global_words` default to empty and must be supplied together. A static declaration is rejected when its outer type resolves to a configured lazy type and its name or a type identifier contains a complete configured semantic word. Camel case and underscores separate words; `AppConfig` contains `config`, while `RECONFIGURE` does not. This avoids treating unrelated registries or a custom type merely named `OnceLock` as ambient configuration. Runtime ambient calls in any static initializer are checked independently.

```rust
use std::sync::OnceLock;
static CONFIG: OnceLock<AppConfig> = OnceLock::new(); // Error.
static REGISTRY: OnceLock<Vec<String>> = OnceLock::new(); // No configuration evidence.
```

Compile-time environment metadata differs from runtime process input. `env!` and `option_env!` are permitted by default. To forbid compile-time capture outside the same boundaries, explicitly add this field to the assertion:

```toml
macros = ["env", "option_env", "std::env", "std::option_env"]
```

Configured macro invocations are reported once; strings inside them are not scanned as runtime calls. Locally declared macros with the same name are not assumed to be the standard macros. Unknown settings, malformed API names, empty functions, and incomplete global policies fail configuration.

A common directive can justify an individual boundary exception:

```rust
// linter:disable rust/environment-variable-access -- Platform fallback captures one input before dependency injection.
std::env::var("SERVICE_URL");
```

Migrates `sources/husklet/src/packages/hl-design-lint/src/rule/rust/environment/`, `sources/prop/packages/design-lint/src/rule/environment/`, and `sources/payment-sdk/packages/design-lint/src/rule/adopted/environment/`. Their alias, path, lazy-global, test-scope, and explicit-boundary cases are covered by Registry tests. Donor differences are explicit configuration: Husklet permits compile-time metadata; Payment-SDK's configured macro variant rejects it. Former hardcoded role directories are now permissions supplied by the repository. Global type evidence uses resolved type identity and complete words instead of substring guesses.
