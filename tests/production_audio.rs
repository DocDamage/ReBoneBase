use rebonebase::production::audio::*;
#[test]
fn trimmed_audio_and_waveform_keep_the_original_source_clock() {
    use rebonebase::animation::{Clip, Playback};
    let mut source = Clip {
        length: 6,
        looped: false,
        ..Default::default()
    };
    source.durations.insert(1, 200);
    source.durations.insert(2, 300);
    let mut clip = source.windowed(3, 5).unwrap();
    assert_eq!(playback_cursor(&clip, 50.), (550, false));
    assert_eq!(playback_cursor(&clip, 350.), (800, false));
    assert_eq!(frame_at_source_time(&clip, 10), 1);
    assert_eq!(frame_at_source_time(&clip, 650), 2);
    assert_eq!(frame_at_source_time(&clip, 1000), 3);
    clip.playback = Playback::Reverse;
    assert_eq!(playback_cursor(&clip, 50.), (750, true));
    assert_eq!(playback_cursor(&clip, 350.), (500, true));
    assert_eq!(frame_at_source_time(&clip, 650), 2);
}
#[test]
fn continuous_audio_cursor_advances_through_holds_and_reverses_without_restarting() {
    let mut clip = rebonebase::animation::Clip {
        length: 2,
        ..Default::default()
    };
    clip.durations.insert(1, 370);
    clip.durations.insert(2, 180);
    assert_eq!(playback_cursor(&clip, 190.), (190, false));
    assert_eq!(playback_cursor(&clip, 390.), (390, false));
    clip.playback = rebonebase::animation::Playback::Reverse;
    assert_eq!(playback_cursor(&clip, 10.), (540, true));
    assert_eq!(playback_cursor(&clip, 190.), (360, true));
    let pcm: Vec<i16> = (0..16)
        .flat_map(|i| if i < 8 { [1000, -1000] } else { [2000, -2000] })
        .collect();
    let track = decode_wav(&encode_wav(&pcm, 8000, 2)).unwrap();
    let reversed = decode_wav(&playback_wav_direction(&track, 2, 2, true))
        .unwrap()
        .samples();
    assert_eq!(&reversed[..2], &[2000, -2000]);
    assert_eq!(&reversed[reversed.len() - 2..], &[1000, -1000]);
}
#[test]
fn pcm_wav_decode_waveform_seek_and_volume_are_bounded() {
    let data = encode_wav(&[0, 32767, -32768, 1000, 0, 2000], 8000, 1);
    let mut track = decode_wav(&data).unwrap();
    assert_eq!(track.duration_ms(), 0);
    assert_eq!(track.samples().len(), 6);
    assert_eq!(waveform(&track, 2).len(), 2);
    track.volume = 0.5;
    let pcm = decode_wav(&playback_wav(&track, 0, 100)).unwrap();
    assert!(pcm.samples()[1] > 16000 && pcm.samples()[1] < 17000);
    assert!(decode_wav(b"not wav").is_err());
    assert!(playback_wav(&track, 10000, 100).is_empty());
}
