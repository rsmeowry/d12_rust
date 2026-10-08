use std::cell::Cell;
use anyhow::{anyhow, ensure};
use windows::Win32::Graphics::Direct3D12::{ID3D12Device, ID3D12Resource, D3D12_RANGE};
use crate::wrappers::graphics::GraphicsDevice;

#[derive(Debug)]
pub struct FrameAllocator {
    buf: ID3D12Resource,
    cpu: *mut u8,
    gpu: u64,
    size: usize,
    offset: Cell<usize>
}

impl FrameAllocator {
    pub fn new(dev: &ID3D12Device, size: usize) -> anyhow::Result<Self> {
        let buf = GraphicsDevice::create_upload_buffer(dev, size, "FrameAllocator")?;
        let mut cpu = std::ptr::null_mut();
        unsafe { buf.Map(0, Some(&D3D12_RANGE::default()), Some(&mut cpu))? };
        let gpu = unsafe { buf.GetGPUVirtualAddress() };
        Ok(Self { buf, cpu: cpu.cast(), gpu, size, offset: Cell::new(0) })
    }

    pub fn reset(&self) {
        self.offset.set(0);
    }

    pub fn resource(&self) -> &ID3D12Resource { &self.buf }

    pub fn try_alloc(&self, size: usize, align: usize) -> Option<(u64, *mut u8)> {
        let start = self.offset.get().next_multiple_of(align);
        let end = start + size;
        if end > self.size {
            return None;
        }
        self.offset.set(end);
        Some((start as u64, unsafe { self.cpu.add(start) }))
    }


    pub fn push_slice<T: bytemuck::Pod>(&self, data: &[T], align: usize) -> anyhow::Result<u64> {
        let bytes: &[u8] = bytemuck::cast_slice(data);
        let (off, ptr) = self.try_alloc(bytes.len(), align)
            .ok_or_else(|| anyhow!("FrameAllocator overflow!"))?;
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
        }
        Ok(self.gpu + off as u64)
    }

    pub fn push<T: bytemuck::Pod>(&self, value: &T) -> anyhow::Result<u64> {
        self.push_slice(std::slice::from_ref(value), 256) // default align is 256 bytes
    }
}