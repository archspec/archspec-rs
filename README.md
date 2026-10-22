<h1>
  <a href="https://github.com/prefix-dev/archspec-rs/">
    <picture>
      <source srcset="https://github.com/prefix-dev/archspec-rs/assets/4995967/b7ecaccb-8421-46cb-926b-5f578a0f09fb" type="image/webp">
      <source srcset="https://github.com/prefix-dev/archspec-rs/assets/4995967/7a12c724-ab7c-43b1-b44c-30a27020a49f" type="image/png">
      <img src="https://github.com/prefix-dev/archspec-rs/assets/4995967/7a12c724-ab7c-43b1-b44c-30a27020a49f" alt="banner">
    </picture>
  </a>
</h1>

<h1 align="center">

[![Crates.io][crates-badge]][crates-url]
![License][license-badge]
[![Build Status][build-badge]][build]
[![Project Chat][chat-badge]][chat-url]
[![docs][docs-badge]][docs-url]

[license-badge]: https://img.shields.io/crates/l/archspec?style=flat-square
[build-badge]: https://img.shields.io/github/actions/workflow/status/prefix-dev/archspec-rs/rust-compile.yml?style=flat-square&branch=main
[build]: https://github.com/prefix-dev/archspec-rs
[chat-badge]: https://img.shields.io/discord/1082332781146800168.svg?label=&logo=discord&logoColor=ffffff&color=7389D8&labelColor=6A7EC2&style=flat-square
[chat-url]: https://discord.gg/kKV8ZxyzY4
[docs-badge]: https://img.shields.io/badge/docs-main-yellow.svg?style=flat-square
[docs-url]: https://docs.rs/archspec
[crates-badge]: https://img.shields.io/crates/v/archspec.svg?style=flat-square
[crates-url]: https://crates.io/crates/archspec

</h1>

# archspec-rs

An implementation of [archspec](https://github.com/archspec/archspec) in Rust.

Archspec aims at providing a standard set of human-understandable labels for various aspects of a system architecture like CPU, network fabrics, etc. and APIs to detect, query and compare them.

The original archspec project grew out of [Spack](https://spack.io/) and is currently under active development. 
At present, it supports APIs to detect and model compatibility relationships among different CPU microarchitectures.

Developed with ❤️ at [prefix.dev](https://prefix.dev).

## Updating the CPU tables

The built-in CPU tables are generated from the `json` submodule and checked into
`src/builtin`. After updating the submodule, run `python3 scripts/generate-tables.py`
(requires `rustfmt`) and commit the generated files. `cargo test -p archspec` checks
that the compiled tables match the JSON.

## License

Licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.
