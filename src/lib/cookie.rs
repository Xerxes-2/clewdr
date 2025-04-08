use colored::Colorize;
use std::collections::{HashMap, HashSet, VecDeque};
use tokio::{
    select,
    sync::{mpsc::Receiver, oneshot},
    time::{Instant, Interval},
};
use tracing::{error, info, warn};

use crate::{
    config::{Config, CookieStatus, Reason, UselessCookie},
    error::ClewdrError,
};

pub struct CookieManager {
    valid: VecDeque<CookieStatus>,
    dispatched: HashMap<CookieStatus, Instant>,
    exhausted: HashSet<CookieStatus>,
    invalid: HashSet<UselessCookie>,
    req_rx: Receiver<oneshot::Sender<Result<CookieStatus, ClewdrError>>>,
    ret_rx: Receiver<(CookieStatus, Option<Reason>)>,
    submit_rx: Receiver<CookieStatus>,
    config: Config,
    interval: Interval,
}

impl CookieStatus {
    /// check if the cookie is expired
    /// if expired, set the reset time to None
    pub fn reset(self) -> Self {
        if let Some(t) = self.reset_time {
            if t < chrono::Utc::now().timestamp() {
                info!("Cookie reset time expired");
                return Self {
                    reset_time: None,
                    ..self
                };
            }
        }
        self
    }
}

impl CookieManager {
    pub fn new(
        mut config: Config,
        req_rx: Receiver<oneshot::Sender<Result<CookieStatus, ClewdrError>>>,
        ret_rx: Receiver<(CookieStatus, Option<Reason>)>,
        submit_rx: Receiver<CookieStatus>,
    ) -> Self {
        config.cookie_array = config.cookie_array.into_iter().map(|c| c.reset()).collect();
        let valid = VecDeque::from_iter(config.cookie_array.iter().filter_map(|c| {
            if c.reset_time.is_none() {
                Some(c.clone())
            } else {
                None
            }
        }));
        let exhaust = HashSet::from_iter(config.cookie_array.iter().filter_map(|c| {
            if c.reset_time.is_some() {
                Some(c.clone())
            } else {
                None
            }
        }));
        let invalid = HashSet::from_iter(config.wasted_cookie.iter().cloned());
        let dispatched = HashMap::new();
        // wait 5 mins to collect unreturned cookies
        let interval = tokio::time::interval(std::time::Duration::from_secs(5 * 60));
        Self {
            valid,
            exhausted: exhaust,
            invalid,
            req_rx,
            config,
            ret_rx,
            submit_rx,
            dispatched,
            interval,
        }
    }

    fn log(&self) {
        info!(
            "Valid: {}, Dispatched: {}, Exhausted: {}, Invalid: {}",
            self.valid.len().to_string().green(),
            self.dispatched.len().to_string().blue(),
            self.exhausted.len().to_string().yellow(),
            self.invalid.len().to_string().red(),
        );
    }

    fn save(&mut self) {
        self.config.cookie_array = self
            .valid
            .iter()
            .chain(self.exhausted.iter())
            .chain(self.dispatched.keys())
            .cloned()
            .collect::<Vec<_>>();
        self.config.wasted_cookie = self.invalid.iter().cloned().collect();
        self.config.save().unwrap_or_else(|e| {
            error!("Failed to save config: {}", e);
        });
    }

    /// Try to dispatch a cookie from the valid set
    fn dispatch(&mut self) -> Result<CookieStatus, ClewdrError> {
        let mut reset_cookies = Vec::new();
        self.exhausted.retain(|cookie| {
            let reset_cookie = cookie.clone().reset();
            if reset_cookie.reset_time.is_none() {
                reset_cookies.push(reset_cookie);
                false
            } else {
                true
            }
        });
        self.valid.extend(reset_cookies);
        self.save();
        // randomly select a cookie from valid cookies and remove it from the set
        let cookie = self
            .valid
            .pop_front()
            .ok_or(ClewdrError::NoCookieAvailable)?;
        let instant = Instant::now();
        self.dispatched.insert(cookie.clone(), instant);
        Ok(cookie)
    }

    /// Collect the cookie and update the state
    fn collect(&mut self, mut cookie: CookieStatus, reason: Option<Reason>) {
        let Some(_) = self.dispatched.remove(&cookie) else {
            return;
        };
        let Some(reason) = reason else {
            self.valid.push_back(cookie);
            return;
        };
        match reason {
            Reason::TooManyRequest(i) => {
                cookie.reset_time = Some(i);
                self.exhausted.insert(cookie);
            }
            Reason::Restricted(i) => {
                cookie.reset_time = Some(i);
                self.exhausted.insert(cookie);
            }
            Reason::NonPro => {
                warn!(
                    "疑似爆米了, id: {}, cookie: {}",
                    cookie.discord.unwrap_or_default().to_string().yellow(),
                    cookie.cookie.to_string().red()
                );
                self.invalid
                    .insert(UselessCookie::new(cookie.cookie, reason));
            }
            r => {
                self.invalid.insert(UselessCookie::new(cookie.cookie, r));
            }
        }
        self.save();
    }

    fn accept(&mut self, cookie: CookieStatus) {
        if self.config.cookie_array.contains(&cookie)
            || self
                .config
                .wasted_cookie
                .iter()
                .any(|c| c.cookie == cookie.cookie)
        {
            warn!("Cookie already exists");
            return;
        }
        self.config.cookie_array.push(cookie.clone());
        self.save();
        self.valid.push_back(cookie.clone());
    }

    /// Run the cookie manager
    /// This function will run in a loop and handle the requests and returns
    /// from the channels
    pub async fn run(mut self) {
        loop {
            self.log();
            select! {
                biased;
                Some((cookie, reason)) = self.ret_rx.recv() => self.collect(cookie, reason),
                Some(cookie) = self.submit_rx.recv() => {
                    self.accept(cookie);
                }
                _ = self.interval.tick() => {
                    // collect cookies that are not returned for 5 mins
                    let now = Instant::now();
                    let expired: Vec<CookieStatus> = self.dispatched
                        .iter()
                        .filter(|(_, time)| now.duration_since(**time).as_secs() > 5 * 60)
                        .map(|(cookie, _)| cookie.clone())
                        .collect();

                    for cookie in expired {
                        warn!("Timing out dispatched cookie: {:?}", cookie);
                        self.dispatched.remove(&cookie);
                        self.valid.push_back(cookie);
                    }
                }
                Some(sender) = self.req_rx.recv() => {
                    let cookie = self.dispatch();
                    if let Err(e) = sender.send(cookie) {
                        error!("Failed to send cookie");
                        if let Ok(c) = e {
                            self.valid.push_back(c);
                        }
                    }
                }
            }
        }
    }
}
