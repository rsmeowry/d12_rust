use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};

#[derive(Debug, Clone)]
pub struct Time {
    secs_per_cnt: f64,
    dt: f64,

    base_time: i64,
    paused_time: i64,
    stop_time: i64,
    prev_time: i64,
    curr_time: i64,

    stopped: bool,
}

impl Time {
    pub fn new() -> anyhow::Result<Self> {
        let mut per_sec: i64 = 0;
        unsafe {
            QueryPerformanceFrequency(&mut per_sec).expect("Failed to query CPU frequency");
        }

        let mut slf = Self {
            secs_per_cnt: 1. / per_sec as f64,
            dt: 0.0,
            base_time: 0,
            paused_time: 0,
            stop_time: 0,
            prev_time: 0,
            curr_time: 0,
            stopped: false,
        };

        slf.reset()?;
        Ok(slf)
    }

    pub fn game_time(&self) -> f32 {
        if self.stopped {
            return ((self.stop_time - self.paused_time - self.base_time) as f64 * self.secs_per_cnt)
                as f32;
        }
        ((self.curr_time - self.stop_time - self.base_time) as f64 * self.secs_per_cnt)
            as f32
    }

    pub fn delta_time(&self) -> f32 {
        self.dt as f32
    }

    pub fn reset(&mut self) -> anyhow::Result<()> {
        let mut c_time: i64 = 0;
        unsafe {
            QueryPerformanceCounter(&mut c_time)?;
        }

        self.base_time = c_time;
        self.prev_time = c_time;
        self.stop_time = 0;
        self.stopped = false;
        Ok(())
    }

    #[allow(unused)]
    pub fn start(&mut self) -> anyhow::Result<()> {
        if !self.stopped {
            return Ok(());
        }

        let mut s_time: i64 = 0;
        unsafe {
            QueryPerformanceCounter(&mut s_time)?;
        }
        self.paused_time += s_time - self.stop_time;
        self.prev_time = s_time;
        self.stop_time = 0;
        self.stopped = false;
        Ok(())
    }

    #[allow(unused)]
    pub fn stop(&mut self) -> anyhow::Result<()> {
        if self.stopped {
            return Ok(());
        }

        let mut c_time: i64 = 0;
        unsafe {
            QueryPerformanceCounter(&mut c_time)?;
        }
        self.stop_time = c_time;
        self.stopped = true;
        Ok(())
    }

    pub fn tick(&mut self) -> anyhow::Result<()> {
        if self.stopped {
            self.dt = 0.;
            return Ok(());
        }

        let mut c_time: i64 = 0;
        unsafe {
            QueryPerformanceCounter(&mut c_time)?;
        }
        self.curr_time = c_time;

        // in case we travel in time
        self.dt = ((self.curr_time - self.prev_time) as f64 * self.secs_per_cnt).max(0.);
        self.prev_time = c_time;

        Ok(())
    }
}
