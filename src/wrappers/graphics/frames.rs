use std::cell::Cell;
use windows::Win32::Graphics::Direct3D12::{ID3D12CommandAllocator, ID3D12Fence};

#[derive(Debug)]
pub struct FrameContext {
    pub command_alloc: ID3D12CommandAllocator,
    pub fence_value: Cell<u64>
}