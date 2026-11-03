//! Video of a meeting: a window the user picks, filmed with its sound and
//! the mic, written by ScreenCaptureKit's own recorder as HEVC in an .mp4.
//! The Mac's video encoder does the compressing; at 15 frames a second a
//! call window comes to a few hundred MB an hour.
//!
//! macOS 15+ (the recorder and the mic option are new there). Elsewhere
//! `pick` reports why and meetings fall back to audio.
//!
//! `pick` shows macOS's own window picker and must be called on the main
//! thread; its answer arrives on the main thread too.

use std::path::Path;

/// The user closed the picker without choosing.
pub const CANCELLED: &str = "cancelled";

/// What the user picked (a window, an app or a display).
pub struct Picked(imp::Filter);

pub type Picking = Box<dyn FnOnce(Result<Picked, String>) + Send>;

/// Show the picker. `done` runs once, with the choice or why there is none.
pub fn pick(done: Picking) {
    imp::pick(done)
}

pub struct VideoRecording(imp::Recording);

impl VideoRecording {
    pub fn start(picked: Picked, path: &Path) -> Result<Self, String> {
        imp::Recording::start(picked.0, path).map(Self)
    }

    /// Stop and wait for the file to be finished.
    pub fn stop(self) -> Result<(), String> {
        self.0.stop()
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::{Picked, Picking, CANCELLED};
    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::runtime::{NSObject, NSObjectProtocol, ProtocolObject};
    use objc2::{define_class, msg_send, AllocAnyThread, DefinedClass, MainThreadMarker, Message};
    use objc2_av_foundation::{AVFileTypeMPEG4, AVVideoCodecTypeHEVC};
    use objc2_core_media::CMTime;
    use objc2_foundation::{NSArray, NSError, NSProcessInfo, NSString, NSURL};
    use objc2_screen_capture_kit::{
        SCContentFilter, SCContentSharingPicker, SCContentSharingPickerConfiguration,
        SCContentSharingPickerMode, SCContentSharingPickerObserver, SCRecordingOutput,
        SCRecordingOutputConfiguration, SCRecordingOutputDelegate, SCStream, SCStreamConfiguration,
    };
    use std::cell::RefCell;
    use std::path::Path;
    use std::sync::mpsc;
    use std::sync::Mutex;
    use std::time::Duration;

    /// The picked content. ScreenCaptureKit filters may be used from any
    /// thread once made.
    pub struct Filter(Retained<SCContentFilter>);
    unsafe impl Send for Filter {}

    fn on_macos_15() -> bool {
        NSProcessInfo::processInfo()
            .operatingSystemVersion()
            .majorVersion
            >= 15
    }

    // --- the picker -----------------------------------------------------------

    pub struct PickerIvars {
        done: Mutex<Option<Picking>>,
    }

    define_class!(
        #[unsafe(super(NSObject))]
        #[name = "LobaPickerObserver"]
        #[ivars = PickerIvars]
        struct PickerObserver;

        unsafe impl NSObjectProtocol for PickerObserver {}

        unsafe impl SCContentSharingPickerObserver for PickerObserver {
            #[unsafe(method(contentSharingPicker:didCancelForStream:))]
            fn did_cancel(&self, _picker: &SCContentSharingPicker, _stream: Option<&SCStream>) {
                self.finish(Err(CANCELLED.into()));
            }

            #[unsafe(method(contentSharingPicker:didUpdateWithFilter:forStream:))]
            fn did_pick(
                &self,
                _picker: &SCContentSharingPicker,
                filter: &SCContentFilter,
                _stream: Option<&SCStream>,
            ) {
                self.finish(Ok(Picked(Filter(filter.retain()))));
            }

            #[unsafe(method(contentSharingPickerStartDidFailWithError:))]
            fn did_fail(&self, error: &NSError) {
                self.finish(Err(error.localizedDescription().to_string()));
            }
        }
    );

    impl PickerObserver {
        fn new(done: Picking) -> Retained<Self> {
            let this = Self::alloc().set_ivars(PickerIvars {
                done: Mutex::new(Some(done)),
            });
            unsafe { msg_send![super(this), init] }
        }

        fn finish(&self, result: Result<Picked, String>) {
            let done = self
                .ivars()
                .done
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .take();
            let picker = unsafe { SCContentSharingPicker::sharedPicker() };
            unsafe {
                picker.removeObserver(ProtocolObject::from_ref(self));
                picker.setActive(false);
            }
            if let Some(done) = done {
                done(result);
            }
        }
    }

    thread_local! {
        // The picker holds its observers weakly; keep ours alive here
        // (picker calls arrive on the main thread, where this lives).
        static OBSERVER: RefCell<Option<Retained<PickerObserver>>> = const { RefCell::new(None) };
    }

    pub fn pick(done: Picking) {
        if MainThreadMarker::new().is_none() {
            done(Err(
                "the window picker has to open on the main thread".into()
            ));
            return;
        }
        if !on_macos_15() {
            done(Err("video recording needs macOS 15 or newer".into()));
            return;
        }
        let observer = PickerObserver::new(done);
        unsafe {
            let picker = SCContentSharingPicker::sharedPicker();
            let config: Retained<SCContentSharingPickerConfiguration> =
                SCContentSharingPickerConfiguration::new();
            config.setAllowedPickerModes(
                SCContentSharingPickerMode::SingleWindow
                    | SCContentSharingPickerMode::SingleApplication
                    | SCContentSharingPickerMode::SingleDisplay,
            );
            // L.O.B.A's own windows aren't what anyone wants to film.
            let me = NSString::from_str("app.loba.desktop");
            config.setExcludedBundleIDs(&NSArray::from_retained_slice(&[me]));
            picker.setDefaultConfiguration(&config);
            picker.addObserver(ProtocolObject::from_ref(&*observer));
            picker.setActive(true);
            OBSERVER.with(|o| *o.borrow_mut() = Some(observer));
            picker.present();
        }
    }

    // --- the recording -------------------------------------------------------

    pub struct RecIvars {
        finished: Mutex<Option<mpsc::Sender<Result<(), String>>>>,
    }

    define_class!(
        #[unsafe(super(NSObject))]
        #[name = "LobaRecordingDelegate"]
        #[ivars = RecIvars]
        struct RecDelegate;

        unsafe impl NSObjectProtocol for RecDelegate {}

        unsafe impl SCRecordingOutputDelegate for RecDelegate {
            #[unsafe(method(recordingOutputDidFinishRecording:))]
            fn did_finish(&self, _output: &SCRecordingOutput) {
                self.send(Ok(()));
            }

            #[unsafe(method(recordingOutput:didFailWithError:))]
            fn did_fail(&self, _output: &SCRecordingOutput, error: &NSError) {
                let msg = error.localizedDescription().to_string();
                log::warn!("Meeting video failed: {msg}");
                self.send(Err(msg));
            }
        }
    );

    impl RecDelegate {
        fn new(tx: mpsc::Sender<Result<(), String>>) -> Retained<Self> {
            let this = Self::alloc().set_ivars(RecIvars {
                finished: Mutex::new(Some(tx)),
            });
            unsafe { msg_send![super(this), init] }
        }

        fn send(&self, r: Result<(), String>) {
            if let Some(tx) = self
                .ivars()
                .finished
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .take()
            {
                let _ = tx.send(r);
            }
        }
    }

    pub struct Recording {
        stream: Retained<SCStream>,
        _output: Retained<SCRecordingOutput>,
        _delegate: Retained<RecDelegate>,
        finished: mpsc::Receiver<Result<(), String>>,
    }

    // Only `stop` touches it again, which ScreenCaptureKit allows from any
    // thread.
    unsafe impl Send for Recording {}

    /// Longest side of the video. Plenty for reading a shared screen, and
    /// keeps a Retina window from doubling the file size.
    const MAX_SIDE: f64 = 1920.0;

    impl Recording {
        pub fn start(filter: Filter, path: &Path) -> Result<Self, String> {
            if !on_macos_15() {
                return Err("video recording needs macOS 15 or newer".into());
            }
            let filter = filter.0;
            let (w, h) = unsafe {
                let rect = filter.contentRect();
                let scale = filter.pointPixelScale() as f64;
                let (w, h) = (rect.size.width * scale, rect.size.height * scale);
                let fit = (MAX_SIDE / w.max(h)).min(1.0);
                // Encoders want even sizes.
                let even = |v: f64| ((v * fit) as usize / 2 * 2).max(2);
                (even(w), even(h))
            };

            let config = unsafe { SCStreamConfiguration::new() };
            unsafe {
                config.setWidth(w);
                config.setHeight(h);
                config.setMinimumFrameInterval(CMTime::new(1, 15));
                config.setShowsCursor(true);
                config.setCapturesAudio(true);
                config.setExcludesCurrentProcessAudio(true);
                config.setSampleRate(48_000);
                config.setChannelCount(2);
                config.setCaptureMicrophone(true);
            }

            let rec_config = unsafe { SCRecordingOutputConfiguration::new() };
            let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
            unsafe {
                rec_config.setOutputURL(&url);
                if let Some(mp4) = AVFileTypeMPEG4 {
                    rec_config.setOutputFileType(mp4);
                }
                if let Some(hevc) = AVVideoCodecTypeHEVC {
                    rec_config.setVideoCodecType(hevc);
                }
            }

            let (tx, finished) = mpsc::channel();
            let delegate = RecDelegate::new(tx);
            let output = unsafe {
                SCRecordingOutput::initWithConfiguration_delegate(
                    SCRecordingOutput::alloc(),
                    &rec_config,
                    ProtocolObject::from_ref(&*delegate),
                )
            };
            let stream = unsafe {
                SCStream::initWithFilter_configuration_delegate(
                    SCStream::alloc(),
                    &filter,
                    &config,
                    None,
                )
            };
            unsafe { stream.addRecordingOutput_error(&output) }
                .map_err(|e| e.localizedDescription().to_string())?;

            let (stx, srx) = mpsc::channel::<Option<String>>();
            let done = RcBlock::new(move |err: *mut NSError| {
                let msg = unsafe { err.as_ref() }.map(|e| e.localizedDescription().to_string());
                let _ = stx.send(msg);
            });
            unsafe { stream.startCaptureWithCompletionHandler(Some(&done)) };
            match srx.recv_timeout(Duration::from_secs(10)) {
                Ok(None) => {}
                Ok(Some(e)) => return Err(e),
                Err(_) => return Err("the screen recorder didn't start".into()),
            }
            log::info!("Meeting video: {w}x{h}, 15 fps, HEVC");
            Ok(Self {
                stream,
                _output: output,
                _delegate: delegate,
                finished,
            })
        }

        pub fn stop(self) -> Result<(), String> {
            let (tx, rx) = mpsc::channel::<()>();
            let done = RcBlock::new(move |_err: *mut NSError| {
                let _ = tx.send(());
            });
            unsafe { self.stream.stopCaptureWithCompletionHandler(Some(&done)) };
            let _ = rx.recv_timeout(Duration::from_secs(5));
            // The file is complete once the recorder says so.
            self.finished
                .recv_timeout(Duration::from_secs(15))
                .unwrap_or_else(|_| Err("the video file wasn't finished in time".into()))
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::Picking;
    use std::path::Path;

    pub struct Filter;
    pub struct Recording;

    pub fn pick(done: Picking) {
        done(Err("video recording is only available on macOS".into()));
    }

    impl Recording {
        pub fn start(_filter: Filter, _path: &Path) -> Result<Self, String> {
            Err("video recording is only available on macOS".into())
        }
        pub fn stop(self) -> Result<(), String> {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_macos_picking_says_why() {
        if cfg!(not(target_os = "macos")) {
            let (tx, rx) = std::sync::mpsc::channel();
            pick(Box::new(move |r| {
                let _ = tx.send(r.is_err());
            }));
            assert!(rx.recv().unwrap());
        }
    }
}
