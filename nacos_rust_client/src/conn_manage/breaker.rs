use std::sync::Arc;

use crate::client::now_millis;


#[derive(Debug,Clone)]
pub enum BreakerStatus {
    Close,
    Open(u64), //半开启时间戳
    HalfOpen(u32,u32), //(连续成功次数,已申请尝试次数)
}

impl Default for BreakerStatus {
    fn default() -> Self {
        Self::Close
    }
}

#[derive(Debug,Clone)]
pub struct BreakerConfig {
    ///失败多少次开启
    pub open_more_than_times: u32,
    ///开启多久后半开启
    pub half_open_after_open_second: u64,
    ///半开启后可访问的比率(分母)
    pub half_open_rate_times: u32,
    ///半开启后成功多少次关闭
    pub close_more_than_times: u32,
}

impl Default for BreakerConfig {
    fn default() -> Self {
        Self { 
            open_more_than_times:2,
            half_open_after_open_second:300,
            half_open_rate_times: 3,
            close_more_than_times: 2,
        }
    }
}

#[derive(Default,Debug,Clone)]
pub struct Breaker{
    pub status:BreakerStatus,
    pub config: Arc<BreakerConfig>,
    pub error_times:u32,
}

impl Breaker {
    pub fn new(status:BreakerStatus,config: Arc<BreakerConfig>) -> Self {
        Self {
            status,
            config,
            error_times:0,
        }
    }

    pub fn error(&mut self) -> &BreakerStatus {
        self.error_times+=1;
        match &self.status {
            BreakerStatus::Close => {
                if self.error_times >= self.config.open_more_than_times {
                    self.status = BreakerStatus::Open(now_millis()+self.config.half_open_after_open_second*1000);
                }
            },
            BreakerStatus::Open(_) => {},
            BreakerStatus::HalfOpen(_,_) => {
                self.status = BreakerStatus::Open(now_millis()+self.config.half_open_after_open_second*1000);
            },
        }
        &self.status
    }

    pub fn success(&mut self) -> &BreakerStatus {
        match &mut self.status {
            BreakerStatus::Close => {
                if self.error_times!=0 {
                    self.error_times=0;
                }
            },
            BreakerStatus::Open(_) => {
                self.status = BreakerStatus::HalfOpen(0,0);
            },
            BreakerStatus::HalfOpen(num,_) => {
                *num +=1;
                if *num >= self.config.close_more_than_times {
                    self.status = BreakerStatus::Close;
                }
            },
        }
        &self.status
    }

    pub fn is_close(&self) -> bool {
        if let BreakerStatus::Close=self.status {
            true
        }
        else{
            false
        }
    }

    pub fn is_open(&self) -> bool {
        if let BreakerStatus::Open(_)=self.status {
            true
        }
        else{
            false
        }
    }

    pub fn is_half_open(&self) -> bool {
        if let BreakerStatus::HalfOpen(_, _)=self.status {
            true
        }
        else{
            false
        }
    }

    pub fn can_try(&mut self) -> bool {
        match &mut self.status {
            BreakerStatus::Close => true,
            BreakerStatus::Open(last_time) => {
                if now_millis() >= *last_time {
                    self.status = BreakerStatus::HalfOpen(0,1);
                }
                false
            },
            BreakerStatus::HalfOpen(_,try_num) => {
                *try_num +=1;
                if *try_num >= self.config.half_open_rate_times{
                    *try_num=0;
                    true
                }
                else{
                    false
                }
            },
        }
    }
}

#[cfg(test)]
mod tests{
    use std::{sync::Arc, time::Duration};

    use crate::conn_manage::breaker::BreakerConfig;

    use super::Breaker;

    #[test]
    fn test_breaker_close_to_open() {
        //let mut breaker = Breaker::default();
        let mut breaker = Breaker::new(Default::default(),Arc::new(BreakerConfig::default()));
        assert!(breaker.is_close());
        breaker.error();
        assert!(breaker.is_close());
        breaker.error();
        assert!(breaker.is_open());
        assert!(!breaker.can_try());
        assert!(breaker.is_open());
    }

    #[test]
    fn test_breaker_open_to_half_open() {
        let config = BreakerConfig{
            half_open_after_open_second:1,
            ..Default::default()
        };
        let mut breaker = Breaker::new(Default::default(),Arc::new(config));
        assert!(breaker.is_close());
        breaker.error();
        assert!(breaker.is_close());
        breaker.error();
        assert!(breaker.is_open());
        assert!(!breaker.is_half_open());

        std::thread::sleep(Duration::from_millis(1000));
        breaker.can_try();
        assert!(breaker.is_half_open());
    }

    #[test]
    fn test_breaker_half_open_to_close() {
        let config = BreakerConfig{
            half_open_after_open_second:1,
            ..Default::default()
        };
        let mut breaker = Breaker::new(Default::default(),Arc::new(config));
        assert!(breaker.is_close());
        breaker.error();
        assert!(breaker.is_close());
        breaker.error();
        assert!(breaker.is_open());
        assert!(!breaker.is_half_open());

        std::thread::sleep(Duration::from_millis(1000));
        breaker.can_try();
        assert!(breaker.is_half_open());

        for _ in 0..10 {
            if breaker.can_try() {
                breaker.success();
            }
        }
        assert!(breaker.is_close());
    }

    #[test]
    fn test_breaker_half_open_to_open() {
        let config = BreakerConfig{
            half_open_after_open_second:1,
            ..Default::default()
        };
        let mut breaker = Breaker::new(Default::default(),Arc::new(config));
        assert!(breaker.is_close());
        breaker.error();
        assert!(breaker.is_close());
        breaker.error();
        assert!(breaker.is_open());
        assert!(!breaker.is_half_open());

        std::thread::sleep(Duration::from_millis(1000));
        breaker.can_try();
        assert!(breaker.is_half_open());

        for _ in 0..10 {
            if breaker.can_try() {
                breaker.success();
                break;
            }
        }
        breaker.error();
        assert!(breaker.is_open());
    }

}
