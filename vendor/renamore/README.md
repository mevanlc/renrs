# Renamore

More ways to rename files.

This copy of the [maintained fork](https://github.com/mevanlc/renamore) is
compiled into renrs as an internal module, so published installs retain its
platform fixes. `Cargo.toml.upstream` records the original package metadata;
this directory is not a separate Cargo package. The root package supplies
`libc` and runs these tests. Linux and Android share the syscall implementation;
this internal copy does not expose the upstream compatibility features.

## Overview

The Rust standard library offers [`std::fs::rename`] for renaming files.
Sometimes, that's not enough. Consider the example of renaming a file but
aborting the operation if something already exists at the destination path.
That can be achieved using the Rust standard library but ensuring that the
operation is atomic requires platform-specific APIs. Without using
platform-specific APIs, a [TOCTTOU] bug can be introduced. This library aims
to provide a cross-platform interface to these APIs.

[`std::fs::rename`]: https://doc.rust-lang.org/std/fs/fn.rename.html
[TOCTTOU]: https://en.wikipedia.org/wiki/Time-of-check_to_time-of-use

## Examples

Renaming a file without the possibility of accidentally overwriting anything
can be done using [`rename_exclusive`]. It should be noted that this feature
is not supported by all combinations of operating system and file system.
`rename_exclusive` will fail if it can't be done atomically.

[`rename_exclusive`]: https://docs.rs/renamore/latest/renamore/fn.rename_exclusive.html

```rust
use std::io::Result;

fn main() -> Result<()> {
    renamore::rename_exclusive("old.txt", "new.txt")
}
```

Alternatively, [`rename_exclusive_fallback`] can be used. This will try to
perform the operation atomically, and use a non-atomic fallback if that's
not supported. The return value will indicate what happened.

[`rename_exclusive_fallback`]: https://docs.rs/renamore/latest/renamore/fn.rename_exclusive_fallback.html

```rust
use std::io::Result;

fn main() -> Result<()> {
    if renamore::rename_exclusive_fallback("old.txt", "new.txt")? {
        // `new.txt` was definitely not overwritten.
        println!("The operation was atomic");
    } else {
        // `new.txt` was probably not overwritten.
        println!("The operation was not atomic");
    }

    Ok(())
}
```

## Platform-specific behaviour

On Linux and Android, the `renameat2` syscall is used through `libc`. Renrs
uses `rename_exclusive` and fails if the exclusive operation is unsupported;
it does not use the non-atomic fallback APIs described above.

## License

Licensed under either of

 * Apache License, Version 2.0
   ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license
   ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
