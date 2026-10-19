use the_memblock::Memblock;
use the_memblock::PhysAddr;

#[derive(PhysAddr)]
#[repr(transparent)]
struct Addr(usize);

fn main() {
    let mut mb = Memblock::<Addr, 4>::new();
    mb.add(Addr(0x1000), Addr(0x1000)).unwrap();
    let p = mb.phys_alloc(Addr(0x100), Addr(0x100)).unwrap();
    assert_eq!(p, Addr(0x1f00));
    assert_eq!(format!("{:?}", p), "Addr(7936)");
}
