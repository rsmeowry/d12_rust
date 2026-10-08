use windows::core::Interface;
use windows::Win32::Graphics::Direct3D12::{ID3D12GraphicsCommandList, ID3D12Resource, D3D12_RANGE};
use crate::wrappers::graphics::GraphicsDevice;

pub struct UploadBatch<'a> {
    gd: &'a GraphicsDevice,
    not_fitting: Vec<ID3D12Resource>
}

impl<'a> UploadBatch<'a> {
    pub fn gd(&self) -> &'a GraphicsDevice {
        self.gd
    }

    pub fn cmd(&self) -> &ID3D12GraphicsCommandList {
        &self.gd.resources().unwrap().upload_list
    }

    pub fn alloc(&mut self, size: usize, align: usize) -> anyhow::Result<(ID3D12Resource, u64, *mut u8)> {
        let staging = &self.gd.resources()?.upload_staging;
        if let Some((off, ptr)) = staging.try_alloc(size, align) {
            return Ok((staging.resource().clone(), off, ptr));
        }
        // did not fit :( need to flush
        self.submit_and_wait()?;
        self.reset()?;
        if let Some((off, ptr)) = staging.try_alloc(size, align) {
            return Ok((staging.resource().clone(), off, ptr));
        }
        // STILL didnt fit, lets create another buffer
        let buf = GraphicsDevice::create_upload_buffer(self.gd.device(), size, "Staging>>not_fitting")?;
        let mut ptr = std::ptr::null_mut();
        unsafe { buf.Map(0, Some(&D3D12_RANGE::default()), Some(&mut ptr))? };
        self.not_fitting.push(buf.clone());
        Ok((buf, 0, ptr.cast()))
    }

    pub fn finish(mut self) -> anyhow::Result<()> {
        self.submit_and_wait()?;
        self.gd.resources()?.upload_open.replace(true);
        Ok(())
    }

    fn submit_and_wait(&mut self) -> anyhow::Result<()> {
        let res = self.gd.resources()?;
        unsafe {
            res.upload_list.Close()?;
            res.command_queue.ExecuteCommandLists(&[Some(res.upload_list.cast()?)]);
        }
        self.gd.flush_command_queue()?;
        self.not_fitting.clear();

        Ok(())
    }

    fn reset(&self) -> anyhow::Result<()> {
        let res = self.gd.resources()?;
        res.upload_staging.reset();
        unsafe {
            res.upload_alloc.Reset()?;
            res.upload_list.Reset(&res.upload_alloc, None)?;
        }
        Ok(())
    }
}

impl GraphicsDevice {
    pub fn begin_upload(&self) -> anyhow::Result<UploadBatch<'_>> {
        let res = self.resources()?;
        anyhow::ensure!(!res.upload_open.replace(true), "Cant open two upload batches at once!");
        res.upload_staging.reset();
        unsafe {
            res.upload_alloc.Reset()?;
            res.upload_list.Reset(&res.upload_alloc, None)?;
        }
        Ok(UploadBatch { gd: self, not_fitting: vec![] })
    }
}