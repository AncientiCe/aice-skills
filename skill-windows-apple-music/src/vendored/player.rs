use std::io::Cursor;

use anyhow::{Context, Result};
use rodio::{Decoder, OutputStream, OutputStreamBuilder, Sink};

pub struct AudioPlayer {
    output_stream: OutputStream,
    sink: Option<Sink>,
}

impl AudioPlayer {
    pub fn new() -> Result<Self> {
        let output_stream = OutputStreamBuilder::open_default_stream()
            .context("failed to create audio output stream")?;
        Ok(Self {
            output_stream,
            sink: None,
        })
    }

    pub async fn play_url(&mut self, url: &str) -> Result<()> {
        self.play_url_with_headers(url, &[]).await
    }

    pub async fn play_url_with_headers(
        &mut self,
        url: &str,
        headers: &[(String, String)],
    ) -> Result<()> {
        self.stop();

        let mut req = reqwest::Client::new().get(url);
        for (k, v) in headers {
            req = req.header(k, v);
        }

        let bytes = req
            .send()
            .await
            .context("failed to request audio stream")?
            .error_for_status()
            .context("audio stream request rejected")?
            .bytes()
            .await
            .context("failed to read audio stream bytes")?;

        let cursor = Cursor::new(bytes.to_vec());
        let source = Decoder::try_from(cursor).context("unsupported audio format")?;
        let sink = Sink::connect_new(self.output_stream.mixer());
        sink.append(source);
        self.sink = Some(sink);
        Ok(())
    }

    pub fn pause(&self) {
        if let Some(sink) = &self.sink {
            sink.pause();
        }
    }

    pub fn resume(&self) {
        if let Some(sink) = &self.sink {
            sink.play();
        }
    }

    pub fn stop(&mut self) {
        if let Some(sink) = self.sink.take() {
            sink.stop();
        }
    }
}
