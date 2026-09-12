use chromiumoxide::{Browser, BrowserConfig};
use futures::StreamExt;
use std::time::Duration;

pub struct Renderer {
    browser: Browser,
    _handler: tokio::task::JoinHandle<()>,
}

impl Renderer {
    pub async fn new() -> Result<Self, String> {
        let (browser, mut handler) = Browser::launch(
            BrowserConfig::builder()
                .new_headless_mode()
                .args(vec![
                    "--no-sandbox",
                    "--disable-gpu",
                    "--disable-dev-shm-usage",
                    "--disable-setuid-sandbox",
                    "--disable-software-rasterizer",
                ])
                .build()
                .map_err(|e| format!("Failed to build browser config: {}", e))?,
        )
        .await
        .map_err(|e| format!("Failed to launch browser: {}", e))?;

        let _handler = tokio::spawn(async move { while handler.next().await.is_some() {} });

        Ok(Self { browser, _handler })
    }

    pub async fn fetch_page(&self, url: &str) -> Result<(String, Option<Vec<u8>>), String> {
        let page = self
            .browser
            .new_page(url)
            .await
            .map_err(|e| format!("Failed to create page: {}", e))?;

        page.wait_for_navigation()
            .await
            .map_err(|e| format!("Navigation failed: {}", e))?;

        tokio::time::sleep(Duration::from_secs(3)).await;

        let html = page
            .content()
            .await
            .map_err(|e| format!("Failed to get page content: {}", e))?;

        let screenshot = page
            .screenshot(
                chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotParams::default(),
            )
            .await
            .ok();

        let _ = page.close().await;
        Ok((html, screenshot))
    }
}
