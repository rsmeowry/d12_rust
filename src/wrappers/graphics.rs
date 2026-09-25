use std::any::Any;
use std::ptr::null;
use windows::Win32::Foundation::{HANDLE, HWND};
use windows::Win32::Graphics::Direct3D12::{D3D12CreateDevice, ID3D12CommandAllocator, ID3D12CommandList, ID3D12CommandQueue, ID3D12Device, ID3D12Device13, ID3D12Fence, ID3D12GraphicsCommandList, D3D12_COMMAND_LIST_TYPE, D3D12_COMMAND_LIST_TYPE_DIRECT, D3D12_COMMAND_QUEUE_DESC, D3D12_COMMAND_QUEUE_FLAG_NONE, D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV, D3D12_DESCRIPTOR_HEAP_TYPE_DSV, D3D12_DESCRIPTOR_HEAP_TYPE_RTV, D3D12_FENCE_FLAG_NONE};
use windows::Win32::Graphics::Direct3D::{D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_12_0};
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory2, IDXGIAdapter, IDXGIFactory, IDXGIFactory6, IDXGISwapChain1, IDXGISwapChain3, DXGI_CREATE_FACTORY_FLAGS, DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_CHAIN_FLAG_ALLOW_MODE_SWITCH, DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING, DXGI_SWAP_EFFECT_FLIP_DISCARD, DXGI_USAGE_RENDER_TARGET_OUTPUT};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT, DXGI_FORMAT_D24_UNORM_S8_UINT, DXGI_FORMAT_R8G8B8A8_UNORM};
use windows::Win32::System::Threading::CreateEventW;

#[derive(Debug, Clone)]
pub struct GraphicsDevice {
    dxgi_factory: IDXGIFactory6,
    device: ID3D12Device,
    resources: Option<GraphicsResources>
}

impl GraphicsDevice {
    const BB_FORMAT: DXGI_FORMAT = DXGI_FORMAT_R8G8B8A8_UNORM;
    const DS_FORMAT: DXGI_FORMAT = DXGI_FORMAT_D24_UNORM_S8_UINT;
    const SWAPCHAIN_BUFFER_COUNT: u32 = 2;

    pub fn new() -> anyhow::Result<Self> {
        let dxgi_factory: IDXGIFactory6 = unsafe { CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0))? };
        let mut device_result: Option<ID3D12Device> = None;
        unsafe {
            if D3D12CreateDevice(None, D3D_FEATURE_LEVEL_11_0, &mut device_result).is_err() {
                // using warp
                let warp: IDXGIAdapter = dxgi_factory.EnumWarpAdapter()?;
                D3D12CreateDevice(&warp, D3D_FEATURE_LEVEL_12_0, &mut device_result)?;
            }
        }

        Ok(Self {
            dxgi_factory,
            device: device_result.ok_or_else(|| anyhow::Error::msg("Failed to create a graphics device!"))?,
            resources: None
        })
    }

    fn resources(&self) -> anyhow::Result<&GraphicsResources> {
        if let Some(r) = &self.resources {
            Ok(r)
        } else {
            Err(anyhow::Error::msg("GraphicsDevice's resources have not yet been initialized"))
        }
    }

    pub fn setup_resources(&mut self, window_handle: HWND, width: u32, height: u32) -> anyhow::Result<()> {
        if self.resources.is_some() {
            return Ok(())
        }

        // fence
        let fence: ID3D12Fence = unsafe { self.device.CreateFence(0, D3D12_FENCE_FLAG_NONE)? };
        let fence_event = unsafe { CreateEventW(None, false, false, None)? };

        // descriptor sizes
        let render_target_desc_size: u32 = unsafe { self.device.GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_RTV) };
        let depth_stencil_desc_size: u32 = unsafe { self.device.GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_DSV) };
        let cbv_srv_uav_desc_size: u32 = unsafe { self.device.GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV) };

        // command objects
        let mut queue_desc = D3D12_COMMAND_QUEUE_DESC::default();
        queue_desc.Type = D3D12_COMMAND_LIST_TYPE_DIRECT;
        queue_desc.Flags = D3D12_COMMAND_QUEUE_FLAG_NONE;
        let command_queue: ID3D12CommandQueue = unsafe { self.device.CreateCommandQueue(&queue_desc)? };
        let command_alloc: ID3D12CommandAllocator = unsafe { self.device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)? };
        let command_list: ID3D12GraphicsCommandList = unsafe { self.device.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, Some(&command_alloc), None)? };
        unsafe { command_list.Close()? };

        // swapchain
        let mut sd = DXGI_SWAP_CHAIN_DESC1::default();
        sd.Width = width;
        sd.Height = height;
        sd.Format = Self::BB_FORMAT;
        sd.SampleDesc.Count = 1;
        sd.SampleDesc.Quality = 0;
        sd.BufferUsage = DXGI_USAGE_RENDER_TARGET_OUTPUT;
        sd.BufferCount = Self::SWAPCHAIN_BUFFER_COUNT;
        sd.SwapEffect = DXGI_SWAP_EFFECT_FLIP_DISCARD;
        sd.Flags = DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING.0 as u32;

        let swapchain1: IDXGISwapChain1 = unsafe { self.dxgi_factory.CreateSwapChainForHwnd(&command_queue, window_handle, &sd, None, None)? };
        let swapchain = unsafe { std::mem::transmute::<IDXGISwapChain1, IDXGISwapChain3>(swapchain1) };
        let bb_index = unsafe { swapchain.GetCurrentBackBufferIndex() };
        self.resources = Some(GraphicsResources {
            fence,
            fence_event,
            render_target_desc_size,
            depth_stencil_desc_size,
            cbv_srv_uav_desc_size,
            command_queue,
            command_alloc,
            command_list,
            swapchain,
            bb_index
        });

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct GraphicsResources {
    fence: ID3D12Fence,
    fence_event: HANDLE,

    render_target_desc_size: u32,
    depth_stencil_desc_size: u32,
    cbv_srv_uav_desc_size: u32,

    command_queue: ID3D12CommandQueue,
    command_alloc: ID3D12CommandAllocator,
    command_list: ID3D12GraphicsCommandList,

    swapchain: IDXGISwapChain3,
    bb_index: u32
}