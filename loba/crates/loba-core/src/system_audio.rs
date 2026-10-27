//! The Mac's own sound output, which during a call is the other people.
//!
//! macOS: ScreenCaptureKit with audio only. Frames arrive as 16 kHz mono
//! float on a private queue and go straight to the caller's sink. Asks for
//! "Screen & System Audio Recording" the first time; without it `start`
//! fails with a message the UI can show. L.O.B.A's own sounds are excluded.
//!
//! Elsewhere `start` returns an error and meetings record the microphone
//! only.

use std::sync::Arc;

/// Receives 16 kHz mono samples. Called on a capture thread: keep it cheap.
pub type Sink = Arc<dyn Fn(&[f32]) + Send + Sync>;

pub const NOT_ALLOWED: &str = "not-allowed";

pub struct SystemAudio {
    inner: imp::Capture,
}

impl SystemAudio {
    /// Start capturing. `Err(NOT_ALLOWED)` when macOS refused permission;
    /// any other `Err` is a message for the log.
    pub fn start(sink: Sink) -> Result<Self, String> {
        imp::Capture::start(sink).map(|inner| Self { inner })
    }

    pub fn stop(self) {
        self.inner.stop();
    }
}

/// Down-mix interleaved or planar float to mono and bring it to 16 kHz.
/// ScreenCaptureKit honours the requested 16 kHz mono, so this is a guard
/// for systems that don't rather than the common path.
pub fn to_mono_16k(data: &[f32], channels: usize, planar: bool, rate: f64) -> Vec<f32> {
    let channels = channels.max(1);
    let frames = data.len() / channels;
    let mono: Vec<f32> = (0..frames)
        .map(|i| {
            (0..channels)
                .map(|c| {
                    if planar {
                        data[c * frames + i]
                    } else {
                        data[i * channels + c]
                    }
                })
                .sum::<f32>()
                / channels as f32
        })
        .collect();
    let ratio = rate / 16_000.0;
    if (ratio - 1.0).abs() < 1e-6 || rate <= 0.0 {
        return mono;
    }
    // Box-filter decimation: fine for speech, which sits well below 8 kHz.
    let out_len = (mono.len() as f64 / ratio) as usize;
    (0..out_len)
        .map(|i| {
            let a = (i as f64 * ratio) as usize;
            let b = (((i + 1) as f64 * ratio) as usize).clamp(a + 1, mono.len());
            mono[a..b].iter().sum::<f32>() / (b - a) as f32
        })
        .collect()
}

#[cfg(target_os = "macos")]
mod imp {
    use super::{to_mono_16k, Sink, NOT_ALLOWED};
    use block2::RcBlock;
    use dispatch2::DispatchQueue;
    use objc2::rc::Retained;
    use objc2::runtime::{NSObject, NSObjectProtocol, ProtocolObject};
    use objc2::{define_class, msg_send, AllocAnyThread, DefinedClass};
    use objc2_core_media::{
        CMAudioFormatDescriptionGetStreamBasicDescription, CMSampleBuffer, CMTime,
    };
    use objc2_foundation::{NSArray, NSError};
    use objc2_screen_capture_kit::{
        SCContentFilter, SCShareableContent, SCStream, SCStreamConfiguration, SCStreamOutput,
        SCStreamOutputType, SCWindow,
    };
    use std::ffi::c_void;
    use std::ptr::NonNull;
    use std::sync::mpsc;
    use std::time::Duration;

    pub struct Ivars {
        sink: Sink,
    }

    define_class!(
        #[unsafe(super(NSObject))]
        #[name = "LobaSystemAudioOutput"]
        #[ivars = Ivars]
        struct Output;

        unsafe impl NSObjectProtocol for Output {}

        unsafe impl SCStreamOutput for Output {
            #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
            fn did_output(
                &self,
                _stream: &SCStream,
                buffer: &CMSampleBuffer,
                kind: SCStreamOutputType,
            ) {
                if kind != SCStreamOutputType::Audio {
                    return;
                }
                if let Some(samples) = unsafe { read_samples(buffer) } {
                    if !samples.is_empty() {
                        (self.ivars().sink)(&samples);
                    }
                }
            }
        }
    );

    impl Output {
        fn new(sink: Sink) -> Retained<Self> {
            let this = Self::alloc().set_ivars(Ivars { sink });
            unsafe { msg_send![super(this), init] }
        }
    }

    const FLAG_NON_INTERLEAVED: u32 = 1 << 5;

    unsafe fn read_samples(buffer: &CMSampleBuffer) -> Option<Vec<f32>> {
        let format = unsafe { buffer.format_description() }?;
        let asbd = unsafe { CMAudioFormatDescriptionGetStreamBasicDescription(&format) };
        if asbd.is_null() {
            return None;
        }
        let asbd = unsafe { &*asbd };
        if asbd.mBitsPerChannel != 32 {
            return None; // ScreenCaptureKit delivers float32; anything else is unexpected
        }
        let block = unsafe { buffer.data_buffer() }?;
        let len = unsafe { block.data_length() };
        if len == 0 {
            return None;
        }
        let mut raw = vec![0f32; len / 4];
        let dest = NonNull::new(raw.as_mut_ptr() as *mut c_void)?;
        if unsafe { block.copy_data_bytes(0, raw.len() * 4, dest) } != 0 {
            return None;
        }
        Some(to_mono_16k(
            &raw,
            asbd.mChannelsPerFrame as usize,
            asbd.mFormatFlags & FLAG_NON_INTERLEAVED != 0,
            asbd.mSampleRate,
        ))
    }

    pub struct Capture {
        stream: Retained<SCStream>,
        _output: Retained<Output>,
    }

    // The stream and output are only touched again by `stop`, which
    // ScreenCaptureKit allows from any thread.
    unsafe impl Send for Capture {}

    fn wait<T>(rx: mpsc::Receiver<T>, what: &str) -> Result<T, String> {
        rx.recv_timeout(Duration::from_secs(10))
            .map_err(|_| format!("{what} timed out"))
    }

    fn error_text(err: *mut NSError) -> Option<String> {
        let err = unsafe { err.as_ref() }?;
        Some(err.localizedDescription().to_string())
    }

    impl Capture {
        pub fn start(sink: Sink) -> Result<Self, String> {
            // 1. What can be captured. Fails when permission was refused.
            let (tx, rx) = mpsc::channel::<Result<usize, String>>();
            let handler =
                RcBlock::new(move |content: *mut SCShareableContent, err: *mut NSError| {
                    let res = match unsafe { Retained::retain(content) } {
                        Some(c) => Ok(Retained::into_raw(c) as usize),
                        None => Err(error_text(err).unwrap_or_else(|| NOT_ALLOWED.into())),
                    };
                    let _ = tx.send(res);
                });
            unsafe { SCShareableContent::getShareableContentWithCompletionHandler(&handler) };
            let raw = wait(rx, "listing displays")?.map_err(|e| {
                log::warn!("System audio unavailable: {e}");
                NOT_ALLOWED.to_string()
            })?;
            let content: Retained<SCShareableContent> =
                unsafe { Retained::from_raw(raw as *mut SCShareableContent) }
                    .ok_or("no shareable content")?;

            // 2. Audio of the whole system, tied to the main display.
            let displays = unsafe { content.displays() };
            let display = displays.firstObject().ok_or("no display to attach to")?;
            let none: Retained<NSArray<SCWindow>> = NSArray::new();
            let filter = unsafe {
                SCContentFilter::initWithDisplay_excludingWindows(
                    SCContentFilter::alloc(),
                    &display,
                    &none,
                )
            };

            // 3. Audio wanted; video kept as small and slow as allowed.
            let config = unsafe { SCStreamConfiguration::new() };
            unsafe {
                config.setCapturesAudio(true);
                config.setSampleRate(16_000);
                config.setChannelCount(1);
                config.setExcludesCurrentProcessAudio(true);
                config.setWidth(2);
                config.setHeight(2);
                config.setMinimumFrameInterval(CMTime::new(1, 1));
            }

            let stream = unsafe {
                SCStream::initWithFilter_configuration_delegate(
                    SCStream::alloc(),
                    &filter,
                    &config,
                    None,
                )
            };
            let output = Output::new(sink);
            let queue = DispatchQueue::new("app.loba.system-audio", None);
            unsafe {
                stream.addStreamOutput_type_sampleHandlerQueue_error(
                    ProtocolObject::from_ref(&*output),
                    SCStreamOutputType::Audio,
                    Some(&queue),
                )
            }
            .map_err(|e| e.localizedDescription().to_string())?;

            // 4. Go.
            let (tx, rx) = mpsc::channel::<Option<String>>();
            let done = RcBlock::new(move |err: *mut NSError| {
                let _ = tx.send(error_text(err));
            });
            unsafe { stream.startCaptureWithCompletionHandler(Some(&done)) };
            if let Some(e) = wait(rx, "starting capture")? {
                log::warn!("System audio capture didn't start: {e}");
                return Err(NOT_ALLOWED.into());
            }
            Ok(Self {
                stream,
                _output: output,
            })
        }

        pub fn stop(self) {
            let (tx, rx) = mpsc::channel::<()>();
            let done = RcBlock::new(move |_err: *mut NSError| {
                let _ = tx.send(());
            });
            unsafe { self.stream.stopCaptureWithCompletionHandler(Some(&done)) };
            let _ = rx.recv_timeout(std::time::Duration::from_secs(3));
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::Sink;

    pub struct Capture;

    impl Capture {
        pub fn start(_sink: Sink) -> Result<Self, String> {
            Err("system audio capture is only available on macOS".into())
        }
        pub fn stop(self) {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mono_16k_passes_through() {
        let x = vec![0.1, 0.2, 0.3];
        assert_eq!(to_mono_16k(&x, 1, false, 16_000.0), x);
    }

    #[test]
    fn stereo_is_averaged_either_layout() {
        // interleaved L R L R
        assert_eq!(
            to_mono_16k(&[1.0, 0.0, 0.5, 0.5], 2, false, 16_000.0),
            vec![0.5, 0.5]
        );
        // planar L L R R
        assert_eq!(
            to_mono_16k(&[1.0, 0.5, 0.0, 0.5], 2, true, 16_000.0),
            vec![0.5, 0.5]
        );
    }

    #[test]
    fn higher_rates_are_brought_down() {
        let x = vec![0.3f32; 48_000];
        let y = to_mono_16k(&x, 1, false, 48_000.0);
        assert_eq!(y.len(), 16_000);
        assert!(y.iter().all(|v| (v - 0.3).abs() < 1e-6));
    }

    #[test]
    fn off_macos_start_says_why() {
        if cfg!(not(target_os = "macos")) {
            assert!(SystemAudio::start(Arc::new(|_| {})).is_err());
        }
    }
}
