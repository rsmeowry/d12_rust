use std::any::Any;
use std::cell::RefCell;
use std::mem::ManuallyDrop;
use std::ptr::null;
use glam::Vec4;
use windows::core::Interface;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, RECT};
use windows::Win32::Graphics::Direct3D12::{D3D12CreateDevice, ID3D12CommandAllocator, ID3D12CommandList, ID3D12CommandQueue, ID3D12DescriptorHeap, ID3D12Device, ID3D12Device13, ID3D12Fence, ID3D12GraphicsCommandList, ID3D12Resource, ID3D12Resource1, D3D12_CLEAR_FLAG_DEPTH, D3D12_CLEAR_FLAG_STENCIL, D3D12_CLEAR_VALUE, D3D12_COMMAND_LIST_TYPE, D3D12_COMMAND_LIST_TYPE_DIRECT, D3D12_COMMAND_QUEUE_DESC, D3D12_COMMAND_QUEUE_FLAG_NONE, D3D12_CPU_DESCRIPTOR_HANDLE, D3D12_DESCRIPTOR_HEAP_DESC, D3D12_DESCRIPTOR_HEAP_FLAG_NONE, D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV, D3D12_DESCRIPTOR_HEAP_TYPE_DSV, D3D12_DESCRIPTOR_HEAP_TYPE_RTV, D3D12_FENCE_FLAG_NONE, D3D12_HEAP_FLAG_NONE, D3D12_HEAP_PROPERTIES, D3D12_HEAP_TYPE_DEFAULT, D3D12_HEAP_TYPE_UPLOAD, D3D12_RESOURCE_BARRIER, D3D12_RESOURCE_BARRIER_0, D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES, D3D12_RESOURCE_BARRIER_FLAG_NONE, D3D12_RESOURCE_BARRIER_TYPE_TRANSITION, D3D12_RESOURCE_DESC, D3D12_RESOURCE_DIMENSION_BUFFER, D3D12_RESOURCE_DIMENSION_TEXTURE2D, D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL, D3D12_RESOURCE_STATES, D3D12_RESOURCE_STATE_COMMON, D3D12_RESOURCE_STATE_DEPTH_WRITE, D3D12_RESOURCE_STATE_GENERIC_READ, D3D12_RESOURCE_STATE_PRESENT, D3D12_RESOURCE_STATE_RENDER_TARGET, D3D12_RESOURCE_TRANSITION_BARRIER, D3D12_TEXTURE_LAYOUT_ROW_MAJOR, D3D12_TEXTURE_LAYOUT_UNKNOWN, D3D12_VIEWPORT};
use windows::Win32::Graphics::Direct3D::{D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_12_0};
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory2, IDXGIAdapter, IDXGIFactory, IDXGIFactory6, IDXGISwapChain1, IDXGISwapChain3, DXGI_CREATE_FACTORY_FLAGS, DXGI_PRESENT_ALLOW_TEARING, DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_CHAIN_FLAG_ALLOW_MODE_SWITCH, DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING, DXGI_SWAP_EFFECT_FLIP_DISCARD, DXGI_USAGE_RENDER_TARGET_OUTPUT};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT, DXGI_FORMAT_D24_UNORM_S8_UINT, DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject, INFINITE};

#[derive(Debug)]
pub struct GraphicsDevice {
    dxgi_factory: IDXGIFactory6,
    device: ID3D12Device,
    current_fence: RefCell<u64>,
    resources: Option<GraphicsResources>
}

impl GraphicsDevice {
    pub const BB_FORMAT: DXGI_FORMAT = DXGI_FORMAT_R8G8B8A8_UNORM;
    pub const DS_FORMAT: DXGI_FORMAT = DXGI_FORMAT_D24_UNORM_S8_UINT;
    pub const SWAPCHAIN_BUFFER_COUNT: u32 = 2;

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
            resources: None,
            current_fence: RefCell::new(0)
        })
    }

    #[inline(always)]
    fn flush_command_queue_internal(&self, command_queue: &ID3D12CommandQueue, fence: &ID3D12Fence, fence_event: HANDLE) -> anyhow::Result<()> {
        unsafe { command_queue.Signal(fence, *self.current_fence.borrow())? };

        unsafe {
            if fence.GetCompletedValue() < *self.current_fence.borrow() {
                fence.SetEventOnCompletion(*self.current_fence.borrow(), fence_event)?;
                WaitForSingleObject(fence_event, INFINITE);
            }
        }

        Ok(())
    }

    #[inline]
    pub fn flush_command_queue(&self) -> anyhow::Result<()> {
        *self.current_fence.borrow_mut() += 1;

        let res = self.resources()?;
        self.flush_command_queue_internal(&res.command_queue, &res.fence, res.fence_event)
    }

    fn resources(&self) -> anyhow::Result<&GraphicsResources> {
        if let Some(r) = &self.resources {
            Ok(r)
        } else {
            Err(anyhow::Error::msg("GraphicsDevice's resources have not yet been initialized"))
        }
    }

    #[inline(always)]
    fn current_bb_handle(&self) -> anyhow::Result<D3D12_CPU_DESCRIPTOR_HANDLE> {
        let res = self.resources()?;

        let mut handle = unsafe { res.rtv_heap.GetCPUDescriptorHandleForHeapStart() };
        handle.ptr += (*res.bb_index.borrow() * res.render_target_desc_size) as usize;

        Ok(handle)
    }

    #[inline(always)]
    fn current_bb(&self) -> anyhow::Result<&ID3D12Resource> {
        let res = self.resources()?;
        Ok(&res.swapchain_buffer[*res.bb_index.borrow() as usize])
    }

    #[inline(always)]
    fn dsv(&self) -> anyhow::Result<D3D12_CPU_DESCRIPTOR_HANDLE> {
        unsafe {
            Ok(self.resources()?.dsv_heap.GetCPUDescriptorHandleForHeapStart())
        }
    }

    pub fn prepare_rt(&self) -> anyhow::Result<()> {
        let res = self.resources()?;

        unsafe {
            res.command_alloc.Reset()?;
            res.command_list.Reset(Some(&res.command_alloc), None)?;
        }

        let barrier = transition_barrier(
            self.current_bb()?,
            D3D12_RESOURCE_STATE_PRESENT,
            D3D12_RESOURCE_STATE_RENDER_TARGET
        );
        let rtv = self.current_bb_handle()?;
        let dsv = self.dsv()?;

        unsafe {
            res.command_list.ResourceBarrier(&[barrier]);
            res.command_list.RSSetViewports(&[res.viewport]);
            res.command_list.RSSetScissorRects(&[res.scissor_rect]);
            res.command_list
                .OMSetRenderTargets(1, Some(&rtv), true, Some(&dsv));
        }

        Ok(())
    }

    pub fn clear_color(&self, color: Vec4) -> anyhow::Result<()> {
        let rtv = self.current_bb_handle()?;
        let dsv = self.dsv()?;

        let res = self.resources()?;
        unsafe {
            res.command_list
                .ClearRenderTargetView(rtv, color.as_ref(), None);
            res.command_list
                .ClearDepthStencilView(dsv, D3D12_CLEAR_FLAG_DEPTH | D3D12_CLEAR_FLAG_STENCIL, 1., 0, None);
        }

        Ok(())
    }

    pub fn device(&self) -> &ID3D12Device {
        &self.device
    }

    pub fn command_list(&self) -> anyhow::Result<&ID3D12GraphicsCommandList> {
        Ok(&self.resources()?.command_list)
    }

    pub fn create_buffer(&self, data: &[u8]) -> anyhow::Result<ID3D12Resource> {
        let heap = D3D12_HEAP_PROPERTIES {
            Type: D3D12_HEAP_TYPE_UPLOAD,
            ..Default::default()
        };
        let desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
            Width: data.len() as u64,
            Height: 1,
            DepthOrArraySize: 1,
            MipLevels: 1,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
            ..Default::default()
        };
        let mut buf: Option<ID3D12Resource> = None;
        unsafe {
            self.device.CreateCommittedResource(
                &heap,
                D3D12_HEAP_FLAG_NONE,
                &desc,
                D3D12_RESOURCE_STATE_GENERIC_READ,
                None,
                &mut buf)?;
        }
        let mut buf = buf.ok_or_else(|| anyhow::Error::msg("Failed to create a buffer"))?;
        unsafe {
            let mut ptr = std::ptr::null_mut();
            buf.Map(0, None, Some(&mut ptr))?;
            std::ptr::copy_nonoverlapping(data.as_ptr(), ptr.cast::<u8>(), data.len());
            buf.Unmap(0, None);
        }

        Ok(buf)
    }

    pub fn display_rt(&self) -> anyhow::Result<()> {
        let res = self.resources()?;
        let barrier = transition_barrier(
            self.current_bb()?,
            D3D12_RESOURCE_STATE_RENDER_TARGET,
            D3D12_RESOURCE_STATE_PRESENT
        );
        unsafe {
            res.command_list.ResourceBarrier(&[barrier]);
        }

        unsafe {
            res.command_list.Close()?
        };

        let command_list = unsafe { std::mem::transmute(res.command_list.clone()) };
        let command_list = &[Some(command_list)];
        unsafe {
            res.command_queue.ExecuteCommandLists(command_list);
        }

        unsafe {
            res.swapchain.Present(0, DXGI_PRESENT_ALLOW_TEARING).ok()?;
            *res.bb_index.borrow_mut() = res.swapchain.GetCurrentBackBufferIndex();
        }

        self.flush_command_queue()?;

        Ok(())
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
        let swapchain = swapchain1.cast::<IDXGISwapChain3>()?;
        let bb_index = unsafe { swapchain.GetCurrentBackBufferIndex() };

        // rtv heap
        let mut rtv_heap_desc = D3D12_DESCRIPTOR_HEAP_DESC::default();
        rtv_heap_desc.NumDescriptors = Self::SWAPCHAIN_BUFFER_COUNT;
        rtv_heap_desc.Type = D3D12_DESCRIPTOR_HEAP_TYPE_RTV;
        rtv_heap_desc.Flags = D3D12_DESCRIPTOR_HEAP_FLAG_NONE;

        let rtv_heap: ID3D12DescriptorHeap = unsafe { self.device.CreateDescriptorHeap(&rtv_heap_desc)? };

        // render view
        let mut rtv_handle = unsafe { rtv_heap.GetCPUDescriptorHandleForHeapStart() };
        let mut swapchain_buffer: Vec<ID3D12Resource> = vec![];
        for buf_idx in 0..Self::SWAPCHAIN_BUFFER_COUNT {
            let buf: ID3D12Resource = unsafe { swapchain.GetBuffer(buf_idx)? };
            swapchain_buffer.push(buf);
            unsafe {
                self.device.CreateRenderTargetView(&swapchain_buffer[buf_idx as usize], None, rtv_handle);
            }

            rtv_handle.ptr += render_target_desc_size as usize;
        }

        // depth+stencil heap
        let mut dsv_heap_desc = D3D12_DESCRIPTOR_HEAP_DESC::default();
        dsv_heap_desc.NumDescriptors = 1;
        dsv_heap_desc.Type = D3D12_DESCRIPTOR_HEAP_TYPE_DSV;
        dsv_heap_desc.Flags = D3D12_DESCRIPTOR_HEAP_FLAG_NONE;
        let dsv_heap: ID3D12DescriptorHeap = unsafe { self.device.CreateDescriptorHeap(&dsv_heap_desc)? };

        // opening alloc to set barrier from dsv
        unsafe { command_list.Reset(&command_alloc, None)? };

        let mut depth_stencil_desc = D3D12_RESOURCE_DESC::default();
        depth_stencil_desc.Dimension = D3D12_RESOURCE_DIMENSION_TEXTURE2D;
        depth_stencil_desc.Alignment = 0;
        depth_stencil_desc.Width = width as u64;
        depth_stencil_desc.Height = height;
        depth_stencil_desc.DepthOrArraySize = 1;
        depth_stencil_desc.MipLevels = 1;
        depth_stencil_desc.Format = Self::DS_FORMAT;
        depth_stencil_desc.SampleDesc.Count = 1;
        depth_stencil_desc.SampleDesc.Quality = 0;
        depth_stencil_desc.Layout = D3D12_TEXTURE_LAYOUT_UNKNOWN;
        depth_stencil_desc.Flags = D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL;

        let mut opt_clear = D3D12_CLEAR_VALUE::default();
        opt_clear.Format = Self::DS_FORMAT;
        unsafe {
            opt_clear.Anonymous.DepthStencil.Depth = 1.;
            opt_clear.Anonymous.DepthStencil.Stencil = 0;
        }
        let mut heap_props = D3D12_HEAP_PROPERTIES::default();
        heap_props.Type = D3D12_HEAP_TYPE_DEFAULT;
        heap_props.CreationNodeMask = 1;
        heap_props.VisibleNodeMask = 1;
        let mut ds_buf_res: Option<ID3D12Resource> = None;
        let depth_stencil_buffer = unsafe {
            self.device.CreateCommittedResource(
                &heap_props,
                D3D12_HEAP_FLAG_NONE,
                &depth_stencil_desc,
                D3D12_RESOURCE_STATE_COMMON,
                Some(&opt_clear), &mut ds_buf_res)?;
            ds_buf_res.ok_or_else(|| anyhow::Error::msg("Failed to create a depth stencil buffer"))
        }?;
        unsafe {
            self.device.CreateDepthStencilView(&depth_stencil_buffer, None, dsv_heap.GetCPUDescriptorHandleForHeapStart());
        }

        let barrier = transition_barrier(&depth_stencil_buffer, D3D12_RESOURCE_STATE_COMMON, D3D12_RESOURCE_STATE_DEPTH_WRITE);
        unsafe { command_list.ResourceBarrier(&[barrier]) };

        unsafe { command_list.Close()? };

        // command lists
        let cmd_list: ID3D12CommandList = unsafe { std::mem::transmute(command_list.clone()) };
        unsafe {
            command_queue.ExecuteCommandLists(&[Some(cmd_list)])
        };

        // saving barrier
        *self.current_fence.get_mut() += 1;
        self.flush_command_queue_internal(&command_queue, &fence, fence_event)?;

        // viewport and scissor
        let mut viewport = D3D12_VIEWPORT {
            TopLeftX: 0.,
            TopLeftY: 0.,
            Width: width as f32,
            Height: height as f32,
            MinDepth: 0.,
            MaxDepth: 1.,
        };

        let mut scissor_rect = RECT {
            left: 0,
            top: 0,
            right: width as i32,
            bottom: height as i32,
        };

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
            bb_index: RefCell::new(bb_index),

            rtv_heap,
            swapchain_buffer,
            dsv_heap,
            depth_stencil_buffer,

            viewport,
            scissor_rect
        });

        Ok(())
    }
}

impl Drop for GraphicsDevice {
    fn drop(&mut self) {
        if let Some(res) = &self.resources {
            unsafe {
                CloseHandle(res.fence_event).expect("Failed to close fence event");
            }
        }
    }
}

#[derive(Debug)]
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
    bb_index: RefCell<u32>,

    rtv_heap: ID3D12DescriptorHeap,
    swapchain_buffer: Vec<ID3D12Resource>,
    dsv_heap: ID3D12DescriptorHeap,
    depth_stencil_buffer: ID3D12Resource,

    viewport: D3D12_VIEWPORT,
    scissor_rect: RECT
}

#[inline(always)]
pub fn transition_barrier(
    res: &ID3D12Resource,
    before: D3D12_RESOURCE_STATES, after
    : D3D12_RESOURCE_STATES) -> D3D12_RESOURCE_BARRIER {
    D3D12_RESOURCE_BARRIER {
        Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
        Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
        Anonymous: D3D12_RESOURCE_BARRIER_0 {
            Transition: ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                pResource: unsafe { std::mem::transmute_copy(res) },
                Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                StateBefore: before,
                StateAfter: after,
            }),
        },
    }
}