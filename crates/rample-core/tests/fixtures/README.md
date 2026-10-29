These hex-encoded MP3 and FLAC fixtures are synthetic sine waves generated for
conversion tests. They contain no user samples. Decode them in tests without
requiring FFmpeg on the user's computer or on CI.

Generation: FFmpeg lavfi sine, 0.12 seconds, 440 Hz / mono / 44.1 kHz / MP3 64 kbps
and 1 kHz / stereo / 48 kHz / FLAC. The FLAC padding metadata block was removed.
