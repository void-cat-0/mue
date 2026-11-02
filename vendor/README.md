# vendor

这里存放从 Cargo 缓存中提取的 `macroquad` 与 `miniquad` 源码副本。

## 为什么有这份副本

上游仓库 `github.com/2278535805/{macroquad,miniquad}` 已被删除（访问返回 404），
`Cargo.toml` 中的 git 依赖无法再拉取。作为临时方案，工作区改为通过 path 依赖
引用本目录，使项目在离线 / 上游仓库不可用时仍可正常构建。

## 来源

| crate | 版本 | 原 git 源 | commit |
| --- | --- | --- | --- |
| `macroquad` | 0.4.16 | `https://github.com/2278535805/macroquad` | `8eb1102a04cc78c9b38e4dfa0485987e4609d7fc` |
| `miniquad` | 0.4.10 | `https://github.com/2278535805/miniquad` | `cccbc8194b8ad77e36d078b11ff01d4e617198d2` |

提取时间：2026-10-26。内容与 `Cargo.lock` 中锁定的 commit 一致，来源为：

- `~/.cargo/git/checkouts/macroquad-cb3749fc0cbb551c/8eb1102`
- `~/.cargo/git/checkouts/miniquad-e8b9893c602b007b/cccbc81`

复制时排除了 `.git` 与 `.cargo-ok`。

## 相对上游的改动

- `vendor/macroquad/Cargo.toml`：`miniquad` 依赖由 git 改为 `path = "../miniquad"`，
  避免再次请求已 404 的仓库。
- 各 crate 入口（`macroquad/src/lib.rs`、`miniquad/src/lib.rs`、`miniquad/build.rs`、
  `macroquad/macroquad_macro/src/lib.rs`）首行加了 `#![allow(warnings)]`。

  原因：Cargo 会给 registry / git 依赖自动加 `--cap-lints allow`，但不会给 path 依赖加。
  改成 path 依赖后，上游代码自带的 `static_mut_refs`、`unexpected_cfgs`、
  `mismatched_lifetime_syntaxes` 等警告会全部冒出来污染构建输出。这些警告与业务代码无关，
  因此在副本内部统一关闭，保持与之前 git 依赖时一致的构建输出。

## 引用方式

根目录 `Cargo.toml` 的 `[workspace.dependencies]` 已改为：

```toml
macroquad = { path = "vendor/macroquad", default-features = false }
miniquad = { path = "vendor/miniquad" }
```

各成员 crate 仍通过 `macroquad.workspace = true` / `miniquad.workspace = true` 使用，
因此无需逐个修改。

## 后续处理

若上游迁移到新地址或出现可用镜像，将根 `Cargo.toml` 的两行改回 git 依赖并删除本目录即可。
