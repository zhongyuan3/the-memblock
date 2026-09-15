# the-memblock-derive

Derive macro for the [`PhysAddr`] trait from
[`the-memblock`](https://docs.rs/the-memblock).

`#[derive(PhysAddr)]` generates a transparent implementation for a
single-field tuple struct wrapping another [`PhysAddr`] implementor, e.g.
`struct Addr(usize)`. All address arithmetic is forwarded to the inner type,
so the overflow-safety and panicking guarantees of [`PhysAddr`] are
preserved.

The derive also implements the supertraits that [`PhysAddr`] requires
(`Clone`, `Copy`, `PartialEq`, `Eq`, `PartialOrd`, `Ord`, `Debug`, `Add`,
`Sub`), so a `#[derive(PhysAddr)] struct Addr(usize);` works out of the box.

Only concrete, non-generic tuple structs with exactly one field are
supported.

## Usage

Add `the-memblock` to your dependencies; the derive macro is re-exported at
the crate root, so there is no need to depend on `the-memblock-derive`
directly.

```rust
use the_memblock::flags::MemblockFlags;
use the_memblock::memblock::Memblock;
use the_memblock::PhysAddr;

#[derive(PhysAddr)]
#[repr(transparent)]
struct Addr(usize);

let mut mb = Memblock::<Addr, 8>::new();
mb.add(Addr(0x1000), Addr(0x1000), MemblockFlags::NONE).unwrap();
let base = mb.phys_alloc(Addr(0x100), Addr(0x100), MemblockFlags::NONE).unwrap();
assert_eq!(base, Addr(0x1f00));
```

## no_std note

A proc-macro crate executes at compile time on the build host and is never
shipped to the target. What matters for no_std targets is the *generated*
code, which is written against `::core` and is therefore no_std-clean.

## License

MIT. See [LICENSE](LICENSE).

[`PhysAddr`]: https://docs.rs/the-memblock/latest/the_memblock/addr/trait.PhysAddr.html