use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct BufferPool {
    inner: Arc<Mutex<BufferPoolInner>>,
}

struct BufferPoolInner {
    max_cached: Option<usize>,
    buf_size: usize,
    storage: Vec<Box<[u8]>>,
    count: usize,
}

impl BufferPool {
    pub fn new(max_cached: Option<usize>, buf_size: usize) -> Self {
        let inner = BufferPoolInner {
            max_cached,
            buf_size,
            storage: Vec::new(),
            count: 0,
        };

        BufferPool {
            inner: Arc::new(Mutex::new(inner)),
        }
    }

    pub fn alloc(&self) -> Buffer {
        let data = {
            let mut pool = self.inner.lock().unwrap();

            if let Some(mut data) = pool.storage.pop() {
                log::trace!(
                    "allocating buffer {:?} from pool ({} total, {} cached)",
                    data.as_ptr(),
                    pool.count,
                    pool.storage.len(),
                );

                data.fill(0);
                data
            } else {
                let data = vec![0; pool.buf_size].into_boxed_slice();
                pool.count += 1;

                log::trace!(
                    "allocating buffer {:?} from global allocator ({} total, {} cached)",
                    data.as_ptr(),
                    pool.count,
                    pool.storage.len(),
                );

                data
            }
        };

        Buffer::new(self.clone(), data)
    }

    fn reclaim(&self, data: Box<[u8]>) {
        let mut pool = self.inner.lock().unwrap();

        if pool.max_cached.is_none() || pool.storage.len() < pool.max_cached.unwrap() {
            log::trace!(
                "reclaiming buffer {:?} ({} total, {} cached)",
                data.as_ptr(),
                pool.count,
                pool.storage.len() + 1,
            );

            pool.storage.push(data);
        } else {
            pool.count -= 1;

            log::trace!(
                "dropping buffer {:?} ({} total, {} cached)",
                data.as_ptr(),
                pool.count,
                pool.storage.len(),
            );

            drop(data);
        }
    }
}

pub struct Buffer {
    pool: BufferPool,
    data: Box<[u8]>,
}

impl Buffer {
    fn new(pool: BufferPool, data: Box<[u8]>) -> Self {
        Self { pool, data }
    }
}

impl Deref for Buffer {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl DerefMut for Buffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

impl AsRef<[u8]> for Buffer {
    fn as_ref(&self) -> &[u8] {
        &self.data
    }
}

impl AsMut<[u8]> for Buffer {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        // note: zero-sized elements in box should not cause allocations
        self.pool.reclaim(std::mem::take(&mut self.data))
    }
}
