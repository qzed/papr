// Heavily based on the tokio intrusively linked list, which can be found at
// the following link:
//
// https://github.com/tokio-rs/tokio/blob/tokio-1.27.0/tokio/src/util/linked_list.rs
//
// Original license (MIT):
//
//  Copyright (c) 2023 Tokio Contributors
//
//  Permission is hereby granted, free of charge, to any
//  person obtaining a copy of this software and associated
//  documentation files (the "Software"), to deal in the
//  Software without restriction, including without
//  limitation the rights to use, copy, modify, merge,
//  publish, distribute, sublicense, and/or sell copies of
//  the Software, and to permit persons to whom the Software
//  is furnished to do so, subject to the following
//  conditions:
//
//  The above copyright notice and this permission notice
//  shall be included in all copies or substantial portions
//  of the Software.
//
//  THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
//  ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
//  TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
//  PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
//  SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
//  CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
//  OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
//  IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
//  DEALINGS IN THE SOFTWARE.

use std::cell::UnsafeCell;
use std::marker::{PhantomData, PhantomPinned};
use std::ptr::NonNull;

/// An intrusive linked list.
pub struct List<L: Link> {
    /// Linked list head
    head: Option<NonNull<L::Node>>,

    /// Linked list tail
    tail: Option<NonNull<L::Node>>,

    /// Node type marker.
    _marker: PhantomData<Box<[L]>>,
}

/// Defines how a type is tracked within a linked list.
///
/// In order to support storing a single type within multiple lists, accessing
/// the list pointers is decoupled from the entry type.
///
/// # Safety
///
/// The `Node` type must be pinned in memory for as long as it is stored inside
/// the list, i.e. the value must not be moved. This is generally fulfilled if
/// the `Pointer` type is a `std::pin::Pin` and `Node` has a `Pointers` member.
pub unsafe trait Link: Sized {
    /// The node type, storing the actual data.
    type Node;

    /// A pointer/handle type pointing to this type of link.
    type Pointer;

    /// Convert the handle to a raw pointer, consuming the pointer but not
    /// dropping its pointed-to object. Similar to Box::into_raw.
    fn into_raw(handle: Self::Pointer) -> NonNull<Self::Node>;

    /// Convert the raw pointer to a handle.
    unsafe fn from_raw(ptr: NonNull<Self::Node>) -> Self::Pointer;

    /// Return the pointers for a node
    ///
    /// # Safety
    ///
    /// The resulting pointer should have the same tag in the stacked-borrows
    /// stack as the argument. In particular, the method may not create an
    /// intermediate reference in the process of creating the resulting raw
    /// pointer.
    unsafe fn pointers(target: NonNull<Self::Node>) -> NonNull<Pointers<Self::Node>>;
}

/// Previous / next pointers for a linked list node.
pub struct Pointers<T> {
    inner: UnsafeCell<PointersInner<T>>,
}

/// We do not want the compiler to put the `noalias` attribute on mutable
/// references to this type, so the type has been made `!Unpin` with a
/// `PhantomPinned` field.
struct PointersInner<T> {
    /// The previous node in the list. null if there is no previous node.
    prev: Option<NonNull<T>>,

    /// The next node in the list. null if there is no previous node.
    next: Option<NonNull<T>>,

    /// This type is !Unpin due to the heuristic from:
    /// <https://github.com/rust-lang/rust/pull/82834>
    ///
    /// In addition: Nodes must not be moved once inserted. Marking pointers as
    /// !Unpin helps to ensure that the wrapping node cannot be (safely) moved.
    _pin: PhantomPinned,
}

impl<L: Link> List<L> {
    /// Creates an empty linked list.
    pub const fn new() -> List<L> {
        List {
            head: None,
            tail: None,
            _marker: PhantomData,
        }
    }

    /// Adds an element first in the list.
    pub fn push_front(&mut self, val: L::Pointer) {
        let ptr = L::into_raw(val);
        assert_ne!(self.head, Some(ptr));
        unsafe {
            L::pointers(ptr).as_mut().set_next(self.head);
            L::pointers(ptr).as_mut().set_prev(None);

            if let Some(head) = self.head {
                L::pointers(head).as_mut().set_prev(Some(ptr));
            }

            self.head = Some(ptr);

            if self.tail.is_none() {
                self.tail = Some(ptr);
            }
        }
    }

    /// Removes the last element from a list and returns it, or None if it is
    /// empty.
    pub fn pop_back(&mut self) -> Option<L::Pointer> {
        unsafe {
            let last = self.tail?;
            self.tail = L::pointers(last).as_ref().get_prev();

            if let Some(prev) = L::pointers(last).as_ref().get_prev() {
                L::pointers(prev).as_mut().set_next(None);
            } else {
                self.head = None
            }

            L::pointers(last).as_mut().set_prev(None);
            L::pointers(last).as_mut().set_next(None);

            Some(L::from_raw(last))
        }
    }

    /// Returns whether the linked list is empty.
    pub fn is_empty(&self) -> bool {
        if self.head.is_some() {
            return false;
        }

        debug_assert!(self.tail.is_none());
        true
    }

    /// Removes the specified node from the list
    ///
    /// # Safety
    ///
    /// The caller **must** ensure that exactly one of the following is true:
    /// - `node` is currently contained by `self`,
    /// - `node` is not contained by any list,
    pub unsafe fn remove(&mut self, node: NonNull<L::Node>) -> Option<L::Pointer> {
        if let Some(prev) = L::pointers(node).as_ref().get_prev() {
            debug_assert_eq!(L::pointers(prev).as_ref().get_next(), Some(node));
            L::pointers(prev)
                .as_mut()
                .set_next(L::pointers(node).as_ref().get_next());
        } else {
            if self.head != Some(node) {
                return None;
            }

            self.head = L::pointers(node).as_ref().get_next();
        }

        if let Some(next) = L::pointers(node).as_ref().get_next() {
            debug_assert_eq!(L::pointers(next).as_ref().get_prev(), Some(node));
            L::pointers(next)
                .as_mut()
                .set_prev(L::pointers(node).as_ref().get_prev());
        } else {
            // This might be the last item in the list
            if self.tail != Some(node) {
                return None;
            }

            self.tail = L::pointers(node).as_ref().get_prev();
        }

        L::pointers(node).as_mut().set_next(None);
        L::pointers(node).as_mut().set_prev(None);

        Some(L::from_raw(node))
    }
}

impl<L: Link> Drop for List<L> {
    fn drop(&mut self) {
        self.head.take();
        let mut last = self.tail.take();

        unsafe {
            while let Some(node) = last {
                let prev = L::pointers(node).as_ref().get_prev();

                // pointer could be some Rc/Arc, so clean up the node and drop the pointer
                L::pointers(node).as_mut().set_prev(None);
                L::pointers(node).as_mut().set_next(None);
                drop(L::from_raw(node));

                last = prev;
            }
        }
    }
}

impl<L: Link> Default for List<L> {
    fn default() -> Self {
        Self::new()
    }
}

impl<L: Link> std::fmt::Debug for List<L> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("List")
            .field("head", &self.head)
            .field("tail", &self.tail)
            .finish()
    }
}

unsafe impl<L: Link> Send for List<L> where L::Node: Send {}
unsafe impl<L: Link> Sync for List<L> where L::Node: Sync {}

impl<T> Pointers<T> {
    pub fn new() -> Self {
        let inner = PointersInner {
            prev: None,
            next: None,
            _pin: PhantomPinned,
        };

        Self {
            inner: UnsafeCell::new(inner),
        }
    }

    fn get_prev(&self) -> Option<NonNull<T>> {
        unsafe { std::ptr::read(std::ptr::addr_of!((*self.inner.get()).prev)) }
    }

    fn get_next(&self) -> Option<NonNull<T>> {
        unsafe { std::ptr::read(std::ptr::addr_of!((*self.inner.get()).next)) }
    }

    fn set_prev(&mut self, value: Option<NonNull<T>>) {
        unsafe { std::ptr::write(std::ptr::addr_of_mut!((*self.inner.get()).prev), value) }
    }

    fn set_next(&mut self, value: Option<NonNull<T>>) {
        unsafe { std::ptr::write(std::ptr::addr_of_mut!((*self.inner.get()).next), value) }
    }
}

impl<T> std::fmt::Debug for Pointers<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let prev = self.get_prev();
        let next = self.get_next();
        f.debug_struct("Pointers")
            .field("prev", &prev)
            .field("next", &next)
            .finish()
    }
}

unsafe impl<T: Send> Send for Pointers<T> {}
unsafe impl<T: Sync> Sync for Pointers<T> {}

#[cfg(test)]
mod test {
    use std::pin::Pin;
    use std::rc::Rc;

    use super::*;

    #[derive(Debug)]
    struct Entry {
        ptr: Pointers<Entry>,
        val: i32,
    }

    unsafe impl<'a> Link for &'a Entry {
        type Node = Entry;
        type Pointer = Pin<&'a Entry>;

        fn into_raw(handle: Pin<&'_ Entry>) -> NonNull<Entry> {
            let ptr = NonNull::from(handle.get_ref());
            std::mem::forget(handle);
            ptr
        }

        unsafe fn from_raw(ptr: NonNull<Entry>) -> Pin<&'a Entry> {
            Pin::new_unchecked(&*ptr.as_ptr())
        }

        unsafe fn pointers(node: NonNull<Entry>) -> NonNull<Pointers<Entry>> {
            let ptrs = std::ptr::addr_of_mut!((*node.as_ptr()).ptr);
            NonNull::new_unchecked(ptrs)
        }
    }

    unsafe impl Link for Rc<Entry> {
        type Node = Entry;
        type Pointer = Pin<Rc<Entry>>;

        fn into_raw(handle: Pin<Rc<Entry>>) -> NonNull<Entry> {
            unsafe {
                let handle = Pin::into_inner_unchecked(handle);
                let handle = Rc::into_raw(handle);
                NonNull::new_unchecked(handle as *mut _)
            }
        }

        unsafe fn from_raw(ptr: NonNull<Entry>) -> Pin<Rc<Entry>> {
            Pin::new_unchecked(Rc::from_raw(ptr.as_ptr()))
        }

        unsafe fn pointers(node: NonNull<Entry>) -> NonNull<Pointers<Entry>> {
            let ptrs = std::ptr::addr_of_mut!((*node.as_ptr()).ptr);
            NonNull::new_unchecked(ptrs)
        }
    }

    fn entry(val: i32) -> Pin<Box<Entry>> {
        let item = Entry {
            ptr: Pointers::new(),
            val,
        };

        Box::pin(item)
    }

    fn entry_rc(val: i32) -> Pin<Rc<Entry>> {
        let item = Entry {
            ptr: Pointers::new(),
            val,
        };

        Rc::pin(item)
    }

    fn ptr(r: &Pin<Box<Entry>>) -> NonNull<Entry> {
        r.as_ref().get_ref().into()
    }

    fn push_all<'a>(list: &mut List<&'a Entry>, entries: &[Pin<&'a Entry>]) {
        for entry in entries.iter() {
            list.push_front(*entry);
        }
    }

    fn collect(list: &mut List<&'_ Entry>) -> Vec<i32> {
        let mut ret = vec![];

        while let Some(entry) = list.pop_back() {
            ret.push(entry.val);
        }

        ret
    }

    macro_rules! assert_clean {
        ($e:ident) => {{
            assert!($e.ptr.get_next().is_none());
            assert!($e.ptr.get_prev().is_none());
        }};
    }

    macro_rules! assert_ptr_eq {
        ($a:expr, $b:expr) => {{
            // Deal with mapping a Pin<&mut T> -> Option<NonNull<T>>
            assert_eq!(Some($a.as_ref().get_ref().into()), $b)
        }};
    }

    #[test]
    fn const_new() {
        const _: List<&Entry> = List::new();
    }

    #[test]
    fn push_and_drain() {
        let a = entry(5);
        let b = entry(7);
        let c = entry(31);

        let mut list = List::new();
        assert!(list.is_empty());

        list.push_front(a.as_ref());
        assert!(!list.is_empty());
        list.push_front(b.as_ref());
        list.push_front(c.as_ref());

        let items: Vec<i32> = collect(&mut list);
        assert_eq!([5, 7, 31].to_vec(), items);

        assert!(list.is_empty());
    }

    #[test]
    fn push_pop_push_pop() {
        let a = entry(5);
        let b = entry(7);

        let mut list: List<&Entry> = List::new();

        list.push_front(a.as_ref());

        let entry = list.pop_back().unwrap();
        assert_eq!(5, entry.val);
        assert!(list.is_empty());

        list.push_front(b.as_ref());

        let entry = list.pop_back().unwrap();
        assert_eq!(7, entry.val);

        assert!(list.is_empty());
        assert!(list.pop_back().is_none());
    }

    #[test]
    fn remove_by_address() {
        let a = entry(5);
        let b = entry(7);
        let c = entry(31);

        unsafe {
            // Remove first
            let mut list = List::new();

            push_all(&mut list, &[c.as_ref(), b.as_ref(), a.as_ref()]);
            assert!(list.remove(ptr(&a)).is_some());
            assert_clean!(a);
            // `a` should be no longer there and can't be removed twice
            assert!(list.remove(ptr(&a)).is_none());
            assert!(!list.is_empty());

            assert!(list.remove(ptr(&b)).is_some());
            assert_clean!(b);
            // `b` should be no longer there and can't be removed twice
            assert!(list.remove(ptr(&b)).is_none());
            assert!(!list.is_empty());

            assert!(list.remove(ptr(&c)).is_some());
            assert_clean!(c);
            // `b` should be no longer there and can't be removed twice
            assert!(list.remove(ptr(&c)).is_none());
            assert!(list.is_empty());
        }

        unsafe {
            // Remove middle
            let mut list = List::new();

            push_all(&mut list, &[c.as_ref(), b.as_ref(), a.as_ref()]);

            assert!(list.remove(ptr(&a)).is_some());
            assert_clean!(a);

            assert_ptr_eq!(b, list.head);
            assert_ptr_eq!(c, b.ptr.get_next());
            assert_ptr_eq!(b, c.ptr.get_prev());

            let items = collect(&mut list);
            assert_eq!([31, 7].to_vec(), items);
        }

        unsafe {
            // Remove middle
            let mut list = List::new();

            push_all(&mut list, &[c.as_ref(), b.as_ref(), a.as_ref()]);

            assert!(list.remove(ptr(&b)).is_some());
            assert_clean!(b);

            assert_ptr_eq!(c, a.ptr.get_next());
            assert_ptr_eq!(a, c.ptr.get_prev());

            let items = collect(&mut list);
            assert_eq!([31, 5].to_vec(), items);
        }

        unsafe {
            // Remove last
            // Remove middle
            let mut list = List::new();

            push_all(&mut list, &[c.as_ref(), b.as_ref(), a.as_ref()]);

            assert!(list.remove(ptr(&c)).is_some());
            assert_clean!(c);

            assert!(b.ptr.get_next().is_none());
            assert_ptr_eq!(b, list.tail);

            let items = collect(&mut list);
            assert_eq!([7, 5].to_vec(), items);
        }

        unsafe {
            // Remove first of two
            let mut list = List::new();

            push_all(&mut list, &[b.as_ref(), a.as_ref()]);

            assert!(list.remove(ptr(&a)).is_some());

            assert_clean!(a);

            // a should be no longer there and can't be removed twice
            assert!(list.remove(ptr(&a)).is_none());

            assert_ptr_eq!(b, list.head);
            assert_ptr_eq!(b, list.tail);

            assert!(b.ptr.get_next().is_none());
            assert!(b.ptr.get_prev().is_none());

            let items = collect(&mut list);
            assert_eq!([7].to_vec(), items);
        }

        unsafe {
            // Remove last of two
            let mut list = List::new();

            push_all(&mut list, &[b.as_ref(), a.as_ref()]);

            assert!(list.remove(ptr(&b)).is_some());

            assert_clean!(b);

            assert_ptr_eq!(a, list.head);
            assert_ptr_eq!(a, list.tail);

            assert!(a.ptr.get_next().is_none());
            assert!(a.ptr.get_prev().is_none());

            let items = collect(&mut list);
            assert_eq!([5].to_vec(), items);
        }

        unsafe {
            // Remove last item
            let mut list = List::new();

            push_all(&mut list, &[a.as_ref()]);

            assert!(list.remove(ptr(&a)).is_some());
            assert_clean!(a);

            assert!(list.head.is_none());
            assert!(list.tail.is_none());
            let items = collect(&mut list);
            assert!(items.is_empty());
        }

        unsafe {
            // Remove missing
            let mut list: List<&Entry> = List::new();

            list.push_front(b.as_ref());
            list.push_front(a.as_ref());

            assert!(list.remove(ptr(&c)).is_none());
        }
    }

    #[test]
    fn drop() {
        let a = entry_rc(5);
        let b = entry_rc(7);
        let c = entry_rc(31);

        // This dance seems to be required to get the refcount for a pinned Rc.
        // Note that this increases the count by one over what we would usually
        // expect.
        //
        // Safety: Since we are just reading the refcount and not moving things
        // around, this is safe.
        unsafe {
            assert_eq!(Rc::strong_count(&Pin::into_inner_unchecked(a.clone())), 2);
            assert_eq!(Rc::strong_count(&Pin::into_inner_unchecked(c.clone())), 2);
            assert_eq!(Rc::strong_count(&Pin::into_inner_unchecked(b.clone())), 2);
        }

        {
            // Create a new list.
            let mut list: List<Rc<Entry>> = List::new();
            list.push_front(a.clone());
            list.push_front(b.clone());
            list.push_front(c.clone());

            // We cloned the elements, so the refcount should be up by one as
            // the list takes ownership.
            //
            // Safety: Since we are just reading the refcount and not moving things
            // around, this is safe.
            unsafe {
                assert_eq!(Rc::strong_count(&Pin::into_inner_unchecked(a.clone())), 3);
                assert_eq!(Rc::strong_count(&Pin::into_inner_unchecked(c.clone())), 3);
                assert_eq!(Rc::strong_count(&Pin::into_inner_unchecked(b.clone())), 3);
            }
        }

        // We just dropped the list. Dropping the list should drop the stored
        // elements and the refcount should be decreased.
        //
        // Safety: Since we are just reading the refcount and not moving things
        // around, this is safe.
        unsafe {
            assert_eq!(Rc::strong_count(&Pin::into_inner_unchecked(a.clone())), 2);
            assert_eq!(Rc::strong_count(&Pin::into_inner_unchecked(c.clone())), 2);
            assert_eq!(Rc::strong_count(&Pin::into_inner_unchecked(b.clone())), 2);
        }

        // We should now have exclusive ownership of the elements again. So we
        // can unwrap them and compare against the original values.

        // Safety: The list has been destroyed, it is now safe to unpin our
        // entries.
        let (a, b, c) = unsafe {
            (
                Pin::into_inner_unchecked(a),
                Pin::into_inner_unchecked(b),
                Pin::into_inner_unchecked(c),
            )
        };

        assert_eq!(Rc::try_unwrap(a).unwrap().val, 5);
        assert_eq!(Rc::try_unwrap(b).unwrap().val, 7);
        assert_eq!(Rc::try_unwrap(c).unwrap().val, 31);
    }
}
