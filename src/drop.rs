use core::{
    cell::{Cell, UnsafeCell},
    marker::PhantomPinned,
    mem::MaybeUninit,
    ptr::NonNull,
};

/// A circular doubly linked list.
#[derive(Debug, Default)]
pub struct DropList {
    pub link: Link,
}

impl DropList {
    /// Safety: `self` must be pinned.
    #[inline]
    pub unsafe fn init(&self) {
        let link_ptr = Some(NonNull::from(&self.link));
        self.link.prev.set(link_ptr);
        self.link.next.set(link_ptr);
    }

    pub unsafe fn insert(&self, node: NonNull<Link>) {
        insert_after(NonNull::from(&self.link), node)
    }

    pub unsafe fn run_drop(&self) {
        let mut curr = self.link.next.get().unwrap();
        let end = NonNull::from(&self.link);
        while curr != end {
            let entry = unsafe { curr.cast::<DropEntry<()>>().as_ref() };
            unsafe {
                (entry.drop_fn)(entry.data.assume_init_ref().get());
            }
            curr = entry.link.next.get().unwrap();
        }
    }
}

#[inline]
unsafe fn insert_after(tail: NonNull<Link>, node_ptr: NonNull<Link>) {
    let tail = tail.as_ref();

    let node = node_ptr.as_ref();
    node.prev.set(Some(NonNull::from(tail)));
    node.next.set(tail.next.get());

    tail.next.get().unwrap().as_ref().prev.set(Some(node_ptr));
    tail.next.set(Some(node_ptr));
}

#[derive(Debug, Default)]
pub struct Link {
    prev: Cell<Option<NonNull<Link>>>,
    next: Cell<Option<NonNull<Link>>>,
    _marker: PhantomPinned,
}

impl Link {
    pub unsafe fn unlink(&self) {
        let Some(prev) = self.prev.take() else {
            return;
        };
        let next = self.next.take().unwrap();
        prev.as_ref().next.set(Some(next));
        next.as_ref().prev.set(Some(prev));
    }
}

#[derive(Debug)]
#[repr(C)]
pub struct DropEntry<T> {
    link: Link,
    drop_fn: unsafe fn(*mut ()),
    data: MaybeUninit<UnsafeCell<T>>,
}

impl<T> DropEntry<T> {
    #[inline]
    pub fn new(val: T) -> Self {
        Self {
            link: Link::default(),
            drop_fn: unsafe {
                core::mem::transmute::<_, unsafe fn(*mut ())>(
                    core::ptr::drop_in_place::<T> as unsafe fn(*mut T),
                )
            },
            data: MaybeUninit::new(UnsafeCell::new(val)),
        }
    }

    #[inline]
    pub unsafe fn link_and_data(&self) -> (NonNull<Link>, *mut T) {
        (NonNull::from(&self.link), self.data.assume_init_ref().get())
    }

    #[inline]
    pub unsafe fn ptr_from_data(data: *mut T) -> NonNull<DropEntry<T>> {
        NonNull::new_unchecked(
            data.byte_sub(memoffset::offset_of!(Self, data))
                .cast::<DropEntry<T>>(),
        )
    }

    #[inline]
    pub unsafe fn link_from_data(data: *mut T) -> NonNull<Link> {
        let entry = Self::ptr_from_data(data).as_ptr();
        NonNull::new_unchecked(core::ptr::addr_of_mut!((*entry).link))
    }
}
