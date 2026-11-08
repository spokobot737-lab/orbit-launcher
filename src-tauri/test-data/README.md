# Synthetic media fixtures

Generated locally using the upstream `eugeneware/ffmpeg-static` b6.1.1 Windows x64 test tool (not bundled with Orbit). These contain only FFmpeg's generated test pattern, no private or third-party footage.

```text
ffmpeg -f lavfi -i testsrc2=size=320x180:rate=15:duration=2 -c:v libx264 -pix_fmt yuv420p -movflags +faststart synthetic.mp4
ffmpeg -f lavfi -i testsrc2=size=160x100:rate=5:duration=1 synthetic.gif
```

The Rust test generates its own 6-megapixel high-entropy PNG. Optional `ORBIT_TEST_FFMPEG` points at a local FFmpeg executable for decoding the transferred MP4 to a null output. No encoder or game assets are distributed in Orbit's installer.
