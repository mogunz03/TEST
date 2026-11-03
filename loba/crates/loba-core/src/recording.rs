//! A meeting's audio recording: your mic and the call, mixed into one track
//! and written as compressed AAC (.m4a), about 15 MB an hour.
//!
//! The two sources arrive on their own threads, each a little early or late
//! against the other. `Mixer` lines them up on the meeting clock and only
//! hands out audio once both have had time to deliver it.
//!
//! macOS: `AudioFile` encodes with Apple's AAC encoder (ExtAudioFile).
//! Elsewhere it refuses to open, and meetings keep only the transcript.

use std::path::Path;

/// Both sources share this rate (16 kHz mono), as transcription does.
pub const RATE: u64 = 16_000;
/// How long the slower source gets to catch up before audio is written.
const HOLD: u64 = 2 * RATE;

/// Sums both sources on the meeting clock (in samples since the start).
#[derive(Default)]
pub struct Mixer {
    /// Meeting sample of `buf[0]`.
    base: u64,
    buf: Vec<f32>,
    /// Furthest sample any source has reached.
    head: u64,
}

impl Mixer {
    /// Add `samples` that started at meeting sample `at`. Anything before
    /// what has already been handed out is too late and is dropped.
    pub fn add(&mut self, at: u64, samples: &[f32]) {
        let skip = self.base.saturating_sub(at) as usize;
        if skip >= samples.len() {
            return;
        }
        let at = at.max(self.base);
        let samples = &samples[skip..];
        let from = (at - self.base) as usize;
        let to = from + samples.len();
        if self.buf.len() < to {
            self.buf.resize(to, 0.0);
        }
        for (d, s) in self.buf[from..to].iter_mut().zip(samples) {
            *d += s;
        }
        self.head = self.head.max(at + samples.len() as u64);
    }

    /// Mixed audio that neither source can still add to.
    pub fn take_ready(&mut self) -> Vec<f32> {
        let until = self.head.saturating_sub(HOLD);
        self.take_until(until)
    }

    /// Everything, at the end of the meeting.
    pub fn take_all(&mut self) -> Vec<f32> {
        let until = self.base + self.buf.len() as u64;
        self.take_until(until)
    }

    fn take_until(&mut self, until: u64) -> Vec<f32> {
        if until <= self.base {
            return Vec::new();
        }
        let n = ((until - self.base) as usize).min(self.buf.len());
        self.base += n as u64;
        // Two voices at once can sum past full scale: soften, don't clip.
        self.buf.drain(..n).map(|s| s.clamp(-1.0, 1.0)).collect()
    }
}

/// An open .m4a being written.
pub struct AudioFile {
    inner: imp::Writer,
}

impl AudioFile {
    pub fn create(path: &Path) -> Result<Self, String> {
        imp::Writer::create(path).map(|inner| Self { inner })
    }

    /// Append 16 kHz mono samples.
    pub fn write(&mut self, samples: &[f32]) -> Result<(), String> {
        if samples.is_empty() {
            return Ok(());
        }
        self.inner.write(samples)
    }

    /// Finish the file. Until this runs the file isn't playable.
    pub fn close(self) {
        self.inner.close();
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::RATE;
    use objc2_audio_toolbox::{
        kAudioFileM4AType, kExtAudioFileProperty_ClientDataFormat, AudioFileFlags,
        ExtAudioFileCreateWithURL, ExtAudioFileDispose, ExtAudioFileRef, ExtAudioFileSetProperty,
        ExtAudioFileWrite,
    };
    use objc2_core_audio_types::{
        kAudioFormatFlagIsFloat, kAudioFormatFlagIsPacked, kAudioFormatLinearPCM,
        kAudioFormatMPEG4AAC, AudioBuffer, AudioBufferList, AudioStreamBasicDescription,
    };
    use objc2_core_foundation::{CFString, CFURLPathStyle, CFURL};
    use std::ffi::c_void;
    use std::path::Path;
    use std::ptr::{self, NonNull};

    pub struct Writer {
        file: ExtAudioFileRef,
    }

    // The file handle is only used from the meeting's worker thread at a
    // time; ExtAudioFile itself has no thread affinity.
    unsafe impl Send for Writer {}

    impl Writer {
        pub fn create(path: &Path) -> Result<Self, String> {
            let path_str = CFString::from_str(&path.to_string_lossy());
            let url = CFURL::with_file_system_path(
                None,
                Some(&path_str),
                CFURLPathStyle::CFURLPOSIXPathStyle,
                false,
            )
            .ok_or("bad recording path")?;

            // On disk: AAC, mono, 16 kHz (the encoder picks the packet size).
            let mut aac = AudioStreamBasicDescription {
                mSampleRate: RATE as f64,
                mFormatID: kAudioFormatMPEG4AAC,
                mFormatFlags: 0,
                mBytesPerPacket: 0,
                mFramesPerPacket: 0,
                mBytesPerFrame: 0,
                mChannelsPerFrame: 1,
                mBitsPerChannel: 0,
                mReserved: 0,
            };
            let mut file: ExtAudioFileRef = ptr::null_mut();
            let status = unsafe {
                ExtAudioFileCreateWithURL(
                    &url,
                    kAudioFileM4AType,
                    NonNull::from(&mut aac),
                    ptr::null(),
                    AudioFileFlags::EraseFile.0,
                    NonNull::from(&mut file),
                )
            };
            if status != 0 || file.is_null() {
                return Err(format!("couldn't create the recording ({status})"));
            }

            // What we hand it: 32-bit float, mono, 16 kHz.
            let mut pcm = AudioStreamBasicDescription {
                mSampleRate: RATE as f64,
                mFormatID: kAudioFormatLinearPCM,
                mFormatFlags: kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked,
                mBytesPerPacket: 4,
                mFramesPerPacket: 1,
                mBytesPerFrame: 4,
                mChannelsPerFrame: 1,
                mBitsPerChannel: 32,
                mReserved: 0,
            };
            let status = unsafe {
                ExtAudioFileSetProperty(
                    file,
                    kExtAudioFileProperty_ClientDataFormat,
                    std::mem::size_of::<AudioStreamBasicDescription>() as u32,
                    NonNull::from(&mut pcm).cast::<c_void>(),
                )
            };
            if status != 0 {
                unsafe { ExtAudioFileDispose(file) };
                return Err(format!(
                    "the AAC encoder refused the audio format ({status})"
                ));
            }
            Ok(Self { file })
        }

        pub fn write(&mut self, samples: &[f32]) -> Result<(), String> {
            let mut list = AudioBufferList {
                mNumberBuffers: 1,
                mBuffers: [AudioBuffer {
                    mNumberChannels: 1,
                    mDataByteSize: (samples.len() * 4) as u32,
                    mData: samples.as_ptr() as *mut c_void,
                }],
            };
            let status = unsafe {
                ExtAudioFileWrite(self.file, samples.len() as u32, NonNull::from(&mut list))
            };
            (status == 0)
                .then_some(())
                .ok_or_else(|| format!("writing the recording failed ({status})"))
        }

        pub fn close(self) {
            unsafe { ExtAudioFileDispose(self.file) };
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use std::path::Path;

    pub struct Writer;

    impl Writer {
        pub fn create(_path: &Path) -> Result<Self, String> {
            Err("audio recording is only available on macOS".into())
        }
        pub fn write(&mut self, _samples: &[f32]) -> Result<(), String> {
            Ok(())
        }
        pub fn close(self) {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_sources_line_up_on_the_meeting_clock() {
        let mut m = Mixer::default();
        m.add(0, &[0.1; 10]);
        m.add(5, &[0.2; 10]); // the call starts 5 samples in
        let out = m.take_all();
        assert_eq!(out.len(), 15);
        assert!((out[0] - 0.1).abs() < 1e-6);
        assert!((out[7] - 0.3).abs() < 1e-6);
        assert!((out[14] - 0.2).abs() < 1e-6);
    }

    #[test]
    fn audio_waits_until_the_slower_source_has_had_its_chance() {
        let mut m = Mixer::default();
        m.add(0, &vec![0.1; (HOLD + 100) as usize]);
        assert_eq!(m.take_ready().len(), 100);
        // the call catches up within the hold and still lands in the mix
        m.add(100, &[0.2; 50]);
        let rest = m.take_all();
        assert!((rest[0] - 0.3).abs() < 1e-6);
    }

    #[test]
    fn audio_too_late_for_the_mix_is_dropped_not_misplaced() {
        let mut m = Mixer::default();
        m.add(0, &vec![0.0; (HOLD + 1000) as usize]);
        assert_eq!(m.take_ready().len(), 1000);
        m.add(900, &[0.5; 200]); // half of it is before what was written
        let rest = m.take_all();
        assert!(
            (rest[0] - 0.5).abs() < 1e-6,
            "the part after 1000 lands at the start"
        );
        assert!((rest[100]).abs() < 1e-6);
    }

    #[test]
    fn loud_overlaps_are_held_to_full_scale() {
        let mut m = Mixer::default();
        m.add(0, &[0.8; 4]);
        m.add(0, &[0.8; 4]);
        assert!(m.take_all().iter().all(|s| *s <= 1.0));
    }

    #[test]
    fn off_macos_recording_says_why() {
        if cfg!(not(target_os = "macos")) {
            assert!(AudioFile::create(Path::new("x.m4a")).is_err());
        }
    }
}
