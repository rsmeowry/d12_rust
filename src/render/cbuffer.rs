use crate::wrappers::graphics::GraphicsDevice;
use windows::Win32::Graphics::Direct3D12::ID3D12Resource;
use rand::random;

#[derive(Debug)]
pub struct ConstBuffer<T: bytemuck::Pod> {
    buf: ID3D12Resource,
    ptr: *mut u8,
    _t: std::marker::PhantomData<T>,
}

impl<T: bytemuck::Pod> ConstBuffer<T> {
    const STRIDE: usize = size_of::<T>().next_multiple_of(256);

    pub fn new(gd: &GraphicsDevice, seed: &T) -> anyhow::Result<Self> {
        let mut bytes = vec![0u8; Self::STRIDE * GraphicsDevice::FRAMES_IN_FLIGHT];
        bytes[..size_of::<T>()].copy_from_slice(bytemuck::bytes_of(seed));
        let buf = gd.create_buffer(&bytes, format!("ConstBuffer_{:4x}", random::<u16>()))?;

        let mut ptr = std::ptr::null_mut();
        unsafe { buf.Map(0, None, Some(&mut ptr))? };
        Ok(Self {
            buf,
            ptr: ptr as *mut u8,
            _t: std::marker::PhantomData::<T>,
        })
    }

    pub fn write(&self, frame: usize, data: &T) {
        let bytes = bytemuck::bytes_of(data);

        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), self.ptr.add(frame * Self::STRIDE), bytes.len())
        };
    }

    pub fn addr(&self, frame: usize) -> u64 {
        unsafe { self.buf.GetGPUVirtualAddress() + (frame * Self::STRIDE) as u64 }
    }
}
