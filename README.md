# Demo: OS sandboxing for unsafe libraries

The [Rust][] compiler vouches only for the safety of code in the safe Rust
dialect. To guard against memory exploits in a C library or `unsafe` Rust code,
we have only the traditional mitigations like [ASLR][] and [no-exec stacks][].

OS-level process sandboxing would provide another, much stronger layer of
protection.  Setting this up traditionally requires lots of platform-specific
code and complex multi-process coordination.  As a result, sandboxing is used
at a coarse-grained level and only in high-value targets like browsers.

This repository is an early proof-of-concept that shows how sandboxing a single
library could be straightforward and convenient.  It runs [`libpng`][libpng]
plus Servo's [very basic Rust wrapper][rust-png] in a sandboxed process, which
receives compressed PNG data on a socket and replies with uncompressed data (or
an error).  Don't expect this to work out of the box on your machine; there are
hard-coded paths among other nonsense.

The sandbox setup is handled by [gaol][], which provides a high-level and
cross-platform interface.  The inter-process procedure calls use [urpc][] and
Rust's `#[derive]` feature.  All together there are about 75 lines of
`libpng`-specific code, on top of Servo's non-sandboxed wrapper.  I expect much
of that remaining code to disappear into reusable libraries with a little more
effort.  Most of the action is in `src/bin/pngbox_daemon.rs`, and the
implementation of `SandboxedDecoder` in `src/lib.rs`.

This library's public interface is exceedingly simple.  You can create a
sandboxed decoder:

```rust
let mut decoder = SandboxedDecoder::new();
```

and then ask it to decode a PNG file:

```rust
let pixels = try!(decoder.decode(file_contents));
```

The security and performance of this approach has not been demonstrated! It's
probably *not* fast enough for image decoding in a browser. For most image
formats, a pure Rust reimplementation would be better.

My top candidate for library sandboxing is [`libpurple`][libpurple], because it

* has a [track record of memory-safety issues][issues],
* doesn't require speedy function calls (for my purposes, anyway), and
* supports a large number of features (protocols, etc.) that would be a real
  pain to re-implement.

That's not to pick on libpurple in particular. Rather, I made this demo because
I think a lot of other libraries are in the same position.

The process of defining the sandbox also produces a non-sandboxed
implementation with the same interface (described by a trait). It should be
straightforward to write code which is generic over the choice of which
particular dependencies to sandbox.  This is resolved at compile time with (in
theory) no added overhead in the "no sandbox" case.

[ASLR]: https://en.wikipedia.org/wiki/Address_space_layout_randomization
[gaol]: https://github.com/pcwalton/gaol
[Rust]: http://www.rust-lang.org/
[urpc]: https://github.com/kmcallister/urpc
[libpng]: http://www.libpng.org/pub/png/libpng.html
[issues]: http://www.pidgin.im/news/security/
[rust-png]: https://github.com/servo/rust-png
[libpurple]: http://www.pidgin.im/
[no-exec stacks]: https://en.wikipedia.org/wiki/NX_bit
