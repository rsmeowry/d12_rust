use std::cell::Cell;
use anyhow::ensure;
use windows::core::HSTRING;
use windows::Win32::Graphics::Direct3D12::{ID3D12DescriptorHeap, ID3D12Device, D3D12_CPU_DESCRIPTOR_HANDLE, D3D12_DESCRIPTOR_HEAP_DESC, D3D12_DESCRIPTOR_HEAP_FLAG_NONE, D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE, D3D12_DESCRIPTOR_HEAP_TYPE, D3D12_DESCRIPTOR_HEAP_TYPE_RTV, D3D12_GPU_DESCRIPTOR_HANDLE};

#[derive(Debug)]
pub struct DescriptorHeap {
    heap: ID3D12DescriptorHeap,
    desc_size: u32,
    capacity: u32,
    next: Cell<u32>,
    shader_visible: bool
}

impl DescriptorHeap {
    pub fn new(dev: &ID3D12Device, ty: D3D12_DESCRIPTOR_HEAP_TYPE, capacity: u32, shader_visible: bool, name: &str) -> anyhow::Result<Self> {
        let desc = D3D12_DESCRIPTOR_HEAP_DESC {
            Type: ty,
            NumDescriptors: capacity,
            Flags: if shader_visible {
                D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE
            } else {
                D3D12_DESCRIPTOR_HEAP_FLAG_NONE
            },
            NodeMask: 0,
        };
        let heap: ID3D12DescriptorHeap = unsafe { dev.CreateDescriptorHeap(&desc)? };
        let size = unsafe {
            heap.SetName(&HSTRING::from(format!("Brine::DescriptorHeap_{}_{:04x}", name, rand::random::<u16>())))?;
            dev.GetDescriptorHandleIncrementSize(ty)
        };
        Ok(Self {
            heap,
            desc_size: size,
            capacity,
            next: Cell::new(0),
            shader_visible,
        })
    }

    pub fn alloc(&self) -> anyhow::Result<u32> {
        let i = self.next.get();
        ensure!(i < self.capacity, "Descriptor heap overflow! {i} > {}", self.capacity);
        self.next.set(i + 1);
        Ok(i)
    }

    pub fn cpu(&self, i: u32) -> D3D12_CPU_DESCRIPTOR_HANDLE {
        let mut h = unsafe { self.heap.GetCPUDescriptorHandleForHeapStart() };
        h.ptr += (i * self.desc_size) as usize;
        h
    }

    pub fn gpu(&self, i: u32) -> D3D12_GPU_DESCRIPTOR_HANDLE {
        let mut h = unsafe { self.heap.GetGPUDescriptorHandleForHeapStart() };
        h.ptr += (i * self.desc_size) as u64;
        h
    }

    pub fn mark(&self) -> u32 {
        self.next.get()
    }
    pub fn rewind(&self, mark: u32) {
        self.next.set(mark);
    }
    pub fn raw(&self) -> &ID3D12DescriptorHeap {
        &self.heap
    }
}