#![allow(unused)]

pub mod frames;

use glam::Vec4;
use std::cell::{Cell, RefCell};
use std::mem::ManuallyDrop;
use std::ops::BitOr;
use anyhow::anyhow;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, RECT};
use windows::Win32::Graphics::Direct3D::{D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_12_0};
use windows::Win32::Graphics::Direct3D12::{D3D12_CLEAR_FLAG_DEPTH, D3D12_CLEAR_FLAG_STENCIL, D3D12_CLEAR_VALUE, D3D12_CLEAR_VALUE_0, D3D12_COMMAND_LIST_TYPE_DIRECT, D3D12_COMMAND_QUEUE_DESC, D3D12_COMMAND_QUEUE_FLAG_NONE, D3D12_CPU_DESCRIPTOR_HANDLE, D3D12_DEPTH_STENCIL_VALUE, D3D12_DESCRIPTOR_HEAP_DESC, D3D12_DESCRIPTOR_HEAP_FLAG_NONE, D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE, D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV, D3D12_DESCRIPTOR_HEAP_TYPE_DSV, D3D12_DESCRIPTOR_HEAP_TYPE_RTV, D3D12_FENCE_FLAG_NONE, D3D12_GPU_DESCRIPTOR_HANDLE, D3D12_HEAP_FLAG_NONE, D3D12_HEAP_PROPERTIES, D3D12_HEAP_TYPE_DEFAULT, D3D12_HEAP_TYPE_UPLOAD, D3D12_RESOURCE_BARRIER, D3D12_RESOURCE_BARRIER_0, D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES, D3D12_RESOURCE_BARRIER_FLAG_NONE, D3D12_RESOURCE_BARRIER_TYPE_TRANSITION, D3D12_RESOURCE_DESC, D3D12_RESOURCE_DIMENSION_BUFFER, D3D12_RESOURCE_DIMENSION_TEXTURE2D, D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL, D3D12_RESOURCE_STATE_COMMON, D3D12_RESOURCE_STATE_DEPTH_WRITE, D3D12_RESOURCE_STATE_GENERIC_READ, D3D12_RESOURCE_STATE_PRESENT, D3D12_RESOURCE_STATE_RENDER_TARGET, D3D12_RESOURCE_STATES, D3D12_RESOURCE_TRANSITION_BARRIER, D3D12_TEXTURE_LAYOUT_ROW_MAJOR, D3D12_TEXTURE_LAYOUT_UNKNOWN, D3D12_VIEWPORT, D3D12CreateDevice, ID3D12CommandAllocator, ID3D12CommandList, ID3D12CommandQueue, ID3D12DescriptorHeap, ID3D12Device, ID3D12Fence, ID3D12GraphicsCommandList, ID3D12Resource, D3D12GetDebugInterface, ID3D12Debug, ID3D12Object, D3D12_DESCRIPTOR_HEAP_TYPE};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT, DXGI_FORMAT_D24_UNORM_S8_UINT, DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory2, DXGI_CREATE_FACTORY_FLAGS, DXGI_PRESENT_ALLOW_TEARING, DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING, DXGI_SWAP_EFFECT_FLIP_DISCARD, DXGI_USAGE_RENDER_TARGET_OUTPUT, IDXGIAdapter, IDXGIFactory6, IDXGISwapChain1, IDXGISwapChain3, DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT};
use windows::Win32::System::Threading::{CreateEventW, INFINITE, WaitForSingleObject};
use windows::core::{Interface, HSTRING, PCWSTR};
use crate::wrappers::graphics::frames::FrameContext;

#[derive(Debug)]
pub struct GraphicsDevice {
    dxgi_factory: IDXGIFactory6,
    device: ID3D12Device,
    current_fence: Cell<u64>,
    resources: Option<GraphicsResources>,
}

impl GraphicsDevice {
    pub const SRV_HEAP_SIZE: u32 = 1024;
    pub const BB_FORMAT: DXGI_FORMAT = DXGI_FORMAT_R8G8B8A8_UNORM;
    pub const DS_FORMAT: DXGI_FORMAT = DXGI_FORMAT_D24_UNORM_S8_UINT;
    pub const SWAPCHAIN_BUFFER_COUNT: u32 = 2;
    pub const FRAMES_IN_FLIGHT: usize = 2;

    pub fn new() -> anyhow::Result<Self> {
        #[cfg(debug_assertions)]
        unsafe {
            let mut interface: Option<ID3D12Debug> = None;
            D3D12GetDebugInterface(&mut interface)?;
            let mut dbg = interface.ok_or_else(|| anyhow!("Failed to get debug interface!"))?;
            dbg.EnableDebugLayer();
        }

        let dxgi_factory: IDXGIFactory6 =
            unsafe { CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0))? };
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
            device: device_result
                .ok_or_else(|| anyhow::Error::msg("Failed to create a graphics device!"))?,
            resources: None,
            current_fence: Cell::new(0),
        })
    }

    fn signal(&self) -> anyhow::Result<u64> {
        let res = self.resources()?;
        let v = self.current_fence.get() + 1;
        self.current_fence.set(v);
        unsafe { res.command_queue.Signal(&res.fence, v)? }
        Ok(v)
    }

    fn wait_for(&self, val: u64) -> anyhow::Result<()> {
        let res = self.resources()?;
        unsafe {
            if res.fence.GetCompletedValue() < val {
                res.fence.SetEventOnCompletion(val, res.fence_event)?;
                WaitForSingleObject(res.fence_event, INFINITE);
            }
        }
        Ok(())
    }

    pub fn flush_command_queue(&self) -> anyhow::Result<()> {
        let v = self.signal()?;
        self.wait_for(v)
    }

    #[inline(always)]
    pub fn frame_index(&self) -> anyhow::Result<usize> {
        Ok(self.resources()?.frame_index.get())
    }

    fn resources_mut(&mut self) -> anyhow::Result<&mut GraphicsResources> {
        if let Some(r) = &mut self.resources {
            Ok(r)
        } else {
            Err(anyhow::Error::msg(
                "GraphicsDevice's resources have not yet been initialized",
            ))
        }
    }

    #[inline(always)]
    fn resources(&self) -> anyhow::Result<&GraphicsResources> {
        if let Some(r) = &self.resources {
            Ok(r)
        } else {
            Err(anyhow::Error::msg(
                "GraphicsDevice's resources have not yet been initialized",
            ))
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
            Ok(self
                .resources()?
                .dsv_heap
                .GetCPUDescriptorHandleForHeapStart())
        }
    }

    pub fn prepare_rt(&self) -> anyhow::Result<()> {
        let res = self.resources()?;

        let frame = res.frames.get(res.frame_index.get()).unwrap();
        self.wait_for(frame.fence_value.get())?;

        unsafe {
            frame.command_alloc.Reset()?;
            res.command_list.Reset(&frame.command_alloc, None)?;
        }

        let barrier = transition_barrier(
            self.current_bb()?,
            D3D12_RESOURCE_STATE_PRESENT,
            D3D12_RESOURCE_STATE_RENDER_TARGET,
        );
        let rtv = self.current_bb_handle()?;
        let dsv = self.dsv()?;

        unsafe {
            res.command_list.ResourceBarrier(&[barrier]);
            res.command_list.RSSetViewports(&[res.viewport]);
            res.command_list.RSSetScissorRects(&[res.scissor_rect]);
            res.command_list
                .SetDescriptorHeaps(&[Some(res.srv_heap.clone())]);
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
            res.command_list.ClearDepthStencilView(
                dsv,
                D3D12_CLEAR_FLAG_DEPTH | D3D12_CLEAR_FLAG_STENCIL,
                1.,
                0,
                None,
            );
        }

        Ok(())
    }

    pub fn device(&self) -> &ID3D12Device {
        &self.device
    }

    pub fn command_list(&self) -> anyhow::Result<&ID3D12GraphicsCommandList> {
        Ok(&self.resources()?.command_list)
    }

    pub fn create_srv(
        &self,
    ) -> anyhow::Result<(D3D12_CPU_DESCRIPTOR_HANDLE, D3D12_GPU_DESCRIPTOR_HANDLE)> {
        let res = self.resources()?;
        let i = res.srv_next.get();
        if i >= Self::SRV_HEAP_SIZE {
            return Err(anyhow::Error::msg("The SRV heap is full!"));
        }

        res.srv_next.set(i + 1);
        let offset = i * res.cbv_srv_uav_desc_size;
        let (mut cpu, mut gpu) = unsafe {
            (
                res.srv_heap.GetCPUDescriptorHandleForHeapStart(),
                res.srv_heap.GetGPUDescriptorHandleForHeapStart(),
            )
        };
        cpu.ptr += offset as usize;
        gpu.ptr += offset as u64;
        Ok((cpu, gpu))
    }

    pub fn execute_now(&self, f: impl FnOnce(&ID3D12GraphicsCommandList)) -> anyhow::Result<()> {
        let res = self.resources()?;
        unsafe {
            res.upload_alloc.Reset()?;
            res.upload_list.Reset(&res.upload_alloc, None)?;
            f(&res.upload_list);
            res.upload_list.Close()?;
            res.command_queue
                .ExecuteCommandLists(&[Some(res.upload_list.cast()?)]);
        }
        self.flush_command_queue()
    }

    pub fn create_buffer<S: AsRef<str>>(&self, data: &[u8], name: S) -> anyhow::Result<ID3D12Resource> {
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
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
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
                &mut buf,
            )?;
        }
        let buf = buf.ok_or_else(|| anyhow::Error::msg("Failed to create a buffer"))?;
        unsafe {
            let mut ptr = std::ptr::null_mut();
            buf.Map(0, None, Some(&mut ptr))?;
            std::ptr::copy_nonoverlapping(data.as_ptr(), ptr.cast::<u8>(), data.len());
            buf.Unmap(0, None);

            // naming for debug
            let name = format!("Brine::{}", name.as_ref());
            let hstring = HSTRING::from(name);
            buf.SetName(&hstring);
        }

        Ok(buf)
    }

    pub fn display_rt(&self) -> anyhow::Result<()> {
        let res = self.resources()?;
        let barrier = transition_barrier(
            self.current_bb()?,
            D3D12_RESOURCE_STATE_RENDER_TARGET,
            D3D12_RESOURCE_STATE_PRESENT,
        );
        unsafe {
            res.command_list.ResourceBarrier(&[barrier]);
        }

        unsafe { res.command_list.Close()? };

        let command_list = unsafe { res.command_list.cast::<ID3D12CommandList>()? };
        let command_list = &[Some(command_list)];
        unsafe {
            res.command_queue.ExecuteCommandLists(command_list);
        }

        unsafe {
            res.swapchain.Present(0, DXGI_PRESENT_ALLOW_TEARING).ok()?;
        }

        let frame = self.frame_context()?;
        frame.fence_value.set(self.signal()?);
        res.frame_index.set((res.frame_index.get() + 1) % Self::FRAMES_IN_FLIGHT);
        unsafe {
            *res.bb_index.borrow_mut() = res.swapchain.GetCurrentBackBufferIndex();
        }

        Ok(())
    }

    pub fn resize(&mut self, width: u32, height: u32) -> anyhow::Result<()> {
        if width == 0 || height == 0 {
            return Ok(()); // minimized
        }
        self.flush_command_queue()?;

        let res = self.resources.as_mut().ok_or_else(|| {
            anyhow::anyhow!("GraphicsDevice's resources have not yet been initialized")
        })?;
        res.swapchain_buffer.clear();

        unsafe {
            res.swapchain.ResizeBuffers(
                Self::SWAPCHAIN_BUFFER_COUNT,
                width,
                height,
                Self::BB_FORMAT,
                DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING,
            )?;
        }
        let (buffers, depth) = Self::size_dependent(
            &self.device,
            &res.swapchain,
            &res.rtv_heap,
            res.render_target_desc_size,
            &res.dsv_heap,
            width,
            height,
        )?;
        res.swapchain_buffer = buffers;
        res.depth_stencil_buffer = depth;
        (res.viewport, res.scissor_rect) = Self::viewport_scissor(width, height);
        *res.bb_index.get_mut() = unsafe { res.swapchain.GetCurrentBackBufferIndex() };

        Ok(())
    }

    pub fn setup_resources(
        &mut self,
        window_handle: HWND,
        width: u32,
        height: u32,
    ) -> anyhow::Result<()> {
        if self.resources.is_some() {
            return Ok(());
        }
        let dev = &self.device;

        // fence
        let fence: ID3D12Fence = unsafe { dev.CreateFence(0, D3D12_FENCE_FLAG_NONE)? };
        let fence_event = unsafe { CreateEventW(None, false, false, None)? };

        // descriptor sizes
        let render_target_desc_size: u32 = unsafe {
            self.device
                .GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_RTV)
        };
        let depth_stencil_desc_size: u32 = unsafe {
            self.device
                .GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_DSV)
        };
        let cbv_srv_uav_desc_size: u32 = unsafe {
            self.device
                .GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV)
        };

        // command objects
        let queue_desc = D3D12_COMMAND_QUEUE_DESC {
            Type: D3D12_COMMAND_LIST_TYPE_DIRECT,
            Flags: D3D12_COMMAND_QUEUE_FLAG_NONE,
            ..Default::default()
        };
        let command_queue: ID3D12CommandQueue = unsafe { dev.CreateCommandQueue(&queue_desc)? };
        let frames = (0..Self::FRAMES_IN_FLIGHT)
            .map(|_| {
                Ok(FrameContext {
                    command_alloc: unsafe { dev.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)? },
                    fence_value: Cell::new(0),
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        // main list for all frames
        let command_list: ID3D12GraphicsCommandList = unsafe {
            dev.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &frames[0].command_alloc, None)?
        };
        unsafe { command_list.Close()? };

        // instant upload alloc and list
        let upload_alloc: ID3D12CommandAllocator =
            unsafe { dev.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)? };
        let upload_list: ID3D12GraphicsCommandList = unsafe {
            dev.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &upload_alloc, None)?
        };
        unsafe { upload_list.Close()? };

        // swapchain
        let sd = DXGI_SWAP_CHAIN_DESC1 {
            Width: width,
            Height: height,
            Format: Self::BB_FORMAT,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: Self::SWAPCHAIN_BUFFER_COUNT,
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
            Flags: DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING.0 as u32,
            ..Default::default()
        };

        let swapchain1: IDXGISwapChain1 = unsafe {
            self.dxgi_factory.CreateSwapChainForHwnd(
                &command_queue,
                window_handle,
                &sd,
                None,
                None,
            )?
        };
        let swapchain = swapchain1.cast::<IDXGISwapChain3>()?;
        let bb_index = unsafe { swapchain.GetCurrentBackBufferIndex() };

        // heaps
        let rtv_heap = Self::create_heap(dev, D3D12_DESCRIPTOR_HEAP_TYPE_RTV, Self::SWAPCHAIN_BUFFER_COUNT, false)?;
        let dsv_heap = Self::create_heap(dev, D3D12_DESCRIPTOR_HEAP_TYPE_DSV, 1, false)?;
        let srv_heap = Self::create_heap(dev, D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV, Self::SRV_HEAP_SIZE, true)?;

        // swapchain
        let (swapchain_buffer, depth_stencil_buffer) = Self::size_dependent(
            &self.device,
            &swapchain,
            &rtv_heap,
            render_target_desc_size,
            &dsv_heap,
            width,
            height,
        )?;
        // viewport and scissor
        let (viewport, scissor_rect) = Self::viewport_scissor(width, height);

        self.resources = Some(GraphicsResources {
            fence,
            fence_event,
            render_target_desc_size,
            depth_stencil_desc_size,
            cbv_srv_uav_desc_size,
            command_queue,
            upload_alloc,
            command_list,
            frames,
            swapchain,
            bb_index: RefCell::new(bb_index),

            rtv_heap,
            swapchain_buffer,
            dsv_heap,
            depth_stencil_buffer,

            srv_heap,
            srv_next: Cell::new(0),

            viewport,
            scissor_rect,
            frame_index: Cell::new(0),
            upload_list,
        });

        Ok(())
    }

    fn create_heap(
        dev: &ID3D12Device,
        ty: D3D12_DESCRIPTOR_HEAP_TYPE,
        count: u32,
        shader_visible: bool,
    ) -> anyhow::Result<ID3D12DescriptorHeap> {
        let desc = D3D12_DESCRIPTOR_HEAP_DESC {
            Type: ty,
            NumDescriptors: count,
            Flags: if shader_visible {
                D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE
            } else {
                D3D12_DESCRIPTOR_HEAP_FLAG_NONE
            },
            NodeMask: 0,
        };
        Ok(unsafe { dev.CreateDescriptorHeap(&desc)? })
    }

    fn frame_context(&self) -> anyhow::Result<&FrameContext> {
        let res = self.resources()?;
        Ok(&res.frames[res.frame_index.get()])
    }

    fn size_dependent(
        dev: &ID3D12Device,
        swapchain: &IDXGISwapChain3,
        rtv_heap: &ID3D12DescriptorHeap,
        rtv_size: u32,
        dsv_heap: &ID3D12DescriptorHeap,
        width: u32,
        height: u32,
    ) -> anyhow::Result<(Vec<ID3D12Resource>, ID3D12Resource)> {
        let mut rtv = unsafe { rtv_heap.GetCPUDescriptorHandleForHeapStart() };
        let mut buffers = Vec::with_capacity(GraphicsDevice::SWAPCHAIN_BUFFER_COUNT as usize);
        for i in 0..GraphicsDevice::SWAPCHAIN_BUFFER_COUNT {
            let buf: ID3D12Resource = unsafe { swapchain.GetBuffer(i)? };
            unsafe { dev.CreateRenderTargetView(&buf, None, rtv) };
            rtv.ptr += rtv_size as usize;
            buffers.push(buf);
        }

        let desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
            Width: width as u64,
            Height: height,
            DepthOrArraySize: 1,
            MipLevels: 1,
            Format: GraphicsDevice::DS_FORMAT,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Layout: D3D12_TEXTURE_LAYOUT_UNKNOWN,
            Flags: D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL,
            ..Default::default()
        };
        let clear = D3D12_CLEAR_VALUE {
            Format: GraphicsDevice::DS_FORMAT,
            Anonymous: D3D12_CLEAR_VALUE_0 {
                DepthStencil: D3D12_DEPTH_STENCIL_VALUE {
                    Depth: 1.,
                    Stencil: 0,
                },
            },
        };
        let heap = D3D12_HEAP_PROPERTIES {
            Type: D3D12_HEAP_TYPE_DEFAULT,
            ..Default::default()
        };
        let mut depth: Option<ID3D12Resource> = None;
        unsafe {
            dev.CreateCommittedResource(
                &heap,
                D3D12_HEAP_FLAG_NONE,
                &desc,
                D3D12_RESOURCE_STATE_DEPTH_WRITE,
                Some(&clear),
                &mut depth,
            )?;
        }
        let depth =
            depth.ok_or_else(|| anyhow::anyhow!("Failed to create a depth stencil buffer!"))?;
        unsafe {
            dev.CreateDepthStencilView(&depth, None, dsv_heap.GetCPUDescriptorHandleForHeapStart())
        };

        Ok((buffers, depth))
    }

    fn viewport_scissor(width: u32, height: u32) -> (D3D12_VIEWPORT, RECT) {
        (
            D3D12_VIEWPORT {
                TopLeftX: 0.0,
                TopLeftY: 0.0,
                Width: width as f32,
                Height: height as f32,
                MinDepth: 0.0,
                MaxDepth: 1.0,
            },
            RECT {
                left: 0,
                top: 0,
                right: width as i32,
                bottom: height as i32,
            },
        )
    }
}

impl Drop for GraphicsDevice {
    fn drop(&mut self) {
        if let Some(res) = &self.resources {
            let _ = self.flush_command_queue();
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
    command_list: ID3D12GraphicsCommandList,
    frames: Vec<FrameContext>,
    frame_index: Cell<usize>,
    upload_alloc: ID3D12CommandAllocator,
    upload_list: ID3D12GraphicsCommandList,

    swapchain: IDXGISwapChain3,
    bb_index: RefCell<u32>,

    rtv_heap: ID3D12DescriptorHeap,
    swapchain_buffer: Vec<ID3D12Resource>,
    dsv_heap: ID3D12DescriptorHeap,
    depth_stencil_buffer: ID3D12Resource,

    srv_heap: ID3D12DescriptorHeap,
    srv_next: Cell<u32>,

    viewport: D3D12_VIEWPORT,
    scissor_rect: RECT,
}

#[inline(always)]
pub fn transition_barrier(
    res: &ID3D12Resource,
    before: D3D12_RESOURCE_STATES,
    after: D3D12_RESOURCE_STATES,
) -> D3D12_RESOURCE_BARRIER {
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