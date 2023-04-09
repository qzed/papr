macro_rules! offset_of {
    ($type:path, $($member:tt)*) => {{
        // Get a temporary object for our pointer calculations
        let tmp = core::mem::MaybeUninit::<$type>::uninit();

        // Get a pointer to that struct
        let ptr_struct = tmp.as_ptr();

        // Get a pointer to the member inside the struct.
        //
        // Safety: `std::ptr::addr_of` does not create an intermediate
        // reference or access `ptr_struct` and its members in any other way.
        // We can therefore safely dereference the pointer here as the pointer
        // itself is valid, just points to uninitialized memory.
        #[allow(unused_unsafe)]
        let ptr_member = unsafe { std::ptr::addr_of!((*ptr_struct).$($member)*) };

        // Compute the offset.
        //
        // Safety: This is safe because both pointers belong to the same
        // allocation and object.
        #[allow(unused_unsafe)]
        unsafe { (ptr_member as *const u8).offset_from(ptr_struct as *const u8) }
    }}
}
pub(crate) use offset_of;

macro_rules! container_of {
    ($ptr:expr, $type:ty, $($f:tt)*) => {{
        // Compute offset from outer struct to the member pointer.
        let offset = $crate::utils::ptr::offset_of!($type, $($f)*);

        // Subtract that offset.
        let ptr = $ptr as *const _ as *const u8;
        ptr.wrapping_offset(-offset) as *const $type
    }}
}
pub(crate) use container_of;

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn offset_of_c() {
        use std::mem::size_of;

        #[repr(C)]
        struct Test {
            a: u32,
            b: u32,
            c: u32,
        }

        let offs_a = 0;
        let offs_b = offs_a + size_of::<u32>() as isize;
        let offs_c = offs_b + size_of::<u32>() as isize;

        assert_eq!(offset_of!(Test, a), offs_a);
        assert_eq!(offset_of!(Test, b), offs_b);
        assert_eq!(offset_of!(Test, c), offs_c);
    }

    #[test]
    fn offset_of_packed() {
        use std::mem::size_of;

        #[repr(C, packed)]
        struct Test {
            a: u8,
            b: u16,
            c: u32,
        }

        let offs_a = 0;
        let offs_b = offs_a + size_of::<u8>() as isize;
        let offs_c = offs_b + size_of::<u16>() as isize;

        assert_eq!(offset_of!(Test, a), offs_a);
        assert_eq!(offset_of!(Test, b), offs_b);
        assert_eq!(offset_of!(Test, c), offs_c);
    }

    #[test]
    fn offset_of_access() {
        struct Test {
            a: u8,
            b: u16,
            c: u32,
        }

        let test = Test {
            a: 0xa5,
            b: 0xf739,
            c: 0x6b28dce1,
        };

        let offs_a = offset_of!(Test, a);
        let offs_b = offset_of!(Test, b);
        let offs_c = offset_of!(Test, c);

        unsafe {
            let ptr_test = &test as *const Test as *const u8;

            let ptr_a: *const u8 = ptr_test.offset(offs_a);
            let ptr_b: *const u16 = ptr_test.offset(offs_b) as *const _;
            let ptr_c: *const u32 = ptr_test.offset(offs_c) as *const _;

            assert_eq!(*ptr_a, test.a);
            assert_eq!(*ptr_b, test.b);
            assert_eq!(*ptr_c, test.c);
        }
    }

    #[test]
    fn container_of() {
        struct Test {
            a: u8,
            b: u16,
            c: u32,
        }

        let test = Test {
            a: 0xa5,
            b: 0xf739,
            c: 0x6b28dce1,
        };

        unsafe {
            let p = &test as *const Test;

            let ptr_a = std::ptr::addr_of!((*p).a);
            let ptr_b = std::ptr::addr_of!((*p).b);
            let ptr_c = std::ptr::addr_of!((*p).c);

            assert_eq!(*ptr_a, test.a);
            assert_eq!(*ptr_b, test.b);
            assert_eq!(*ptr_c, test.c);

            assert!(std::ptr::eq(p, container_of!(ptr_a, Test, a)));
            assert!(std::ptr::eq(p, container_of!(ptr_b, Test, b)));
            assert!(std::ptr::eq(p, container_of!(ptr_c, Test, c)));
        }
    }
}
