# Troubleshooting Guide

**LoLShorts v1.2.0** - Solutions to Common Issues

---

## Table of Contents

1. [Installation & Setup Issues](#installation--setup-issues)
2. [Recording Issues](#recording-issues)
3. [Auto-Edit Issues](#auto-edit-issues)
4. [Canvas & Graphics Issues](#canvas--graphics-issues)
5. [Audio Issues](#audio-issues)
6. [Performance Issues](#performance-issues)
7. [Export & File Issues](#export--file-issues)
8. [League Client (LCU) Issues](#league-client-lcu-issues)
9. [Error Messages Reference](#error-messages-reference)
10. [Getting Help](#getting-help)

---

## Installation & Setup Issues

### "FFmpeg is not installed or not found in system PATH"

**Cause**: FFmpeg is missing or not added to Windows PATH.

**Solution**:

1. **Download FFmpeg**:
   - Go to [ffmpeg.org/download.html](https://ffmpeg.org/download.html)
   - Click "Windows builds from gyan.dev"
   - Download "ffmpeg-release-essentials.zip"

2. **Extract FFmpeg**:
   - Extract to `C:\ffmpeg\`
   - Folder should contain `bin\ffmpeg.exe`

3. **Add to PATH**:
   ```
   1. Open Windows Search, type "Environment Variables"
   2. Click "Edit the system environment variables"
   3. Click "Environment Variables" button
   4. Under "System variables", find "Path"
   5. Click "Edit" → "New"
   6. Add: C:\ffmpeg\bin
   7. Click "OK" on all windows
   ```

4. **Verify Installation**:
   - Open Command Prompt (cmd)
   - Type: `ffmpeg -version`
   - Should see FFmpeg version info

5. **Restart LoLShorts**

**Still not working?**
- Ensure folder path is exactly `C:\ffmpeg\bin` (not `C:\ffmpeg\bin\bin`)
- Restart your computer after adding to PATH
- Check Windows Defender didn't quarantine ffmpeg.exe

---

### "Application won't start / crashes on launch"

**Symptoms**: LoLShorts shows error on startup or closes immediately.

**Solutions**:

**1. Check System Requirements**:
- Windows 10 version 1809 or later
- .NET Runtime 6.0 or later
- 4GB RAM minimum
- 2GB free disk space

**2. Run as Administrator**:
- Right-click LoLShorts icon
- Select "Run as administrator"
- Check if issue persists

**3. Check Logs**:
- Navigate to: `C:\Users\[You]\AppData\Local\LoLShorts\logs\`
- Open most recent `app.log` file
- Look for error messages (share in Discord for help)

**4. Reinstall**:
- Uninstall LoLShorts
- Delete: `C:\Users\[You]\AppData\Local\LoLShorts\`
- Download latest version
- Install fresh

---

### "Can't connect to database"

**Cause**: Database file is corrupted or locked.

**Solution**:

1. **Close LoLShorts completely**
2. **Navigate to**: `C:\Users\[You]\AppData\Local\LoLShorts\`
3. **Rename**: `lolshorts.db` to `lolshorts.db.backup`
4. **Restart LoLShorts** (new database created automatically)
5. **Restore data** (if needed):
   - Use database recovery tool (coming soon)
   - Report issue on GitHub for assistance

---

## Recording Issues

### "Recording not starting when game starts"

**Symptoms**: League game starts, but LoLShorts doesn't begin recording.

**Causes & Solutions**:

**1. League Client Not Connected**:
- ✅ Check LCU status indicator (top-right of LoLShorts)
- ✅ Ensure League Client is running BEFORE starting LoLShorts
- ✅ Restart LoLShorts after launching League

**2. Recording Settings Disabled**:
- ✅ Go to Settings → Recording
- ✅ Ensure "Auto-Start Recording" is enabled
- ✅ Check "Monitor League Client" is enabled

**3. Game DVR Not Available**:
- ✅ Open Xbox Game Bar (Windows + G)
- ✅ Go to Settings → Capturing
- ✅ Enable "Record in the background"
- ✅ Verify storage location has space

**4. Permissions Issue**:
- ✅ Run LoLShorts as administrator
- ✅ Allow all Windows permissions (Microphone, Storage)

---

### "Recording file is 0 bytes / corrupted"

**Cause**: Recording interrupted or insufficient disk space.

**Solution**:

1. **Check Disk Space**:
   - Ensure at least 10GB free on recording drive
   - Recordings use ~500MB per 30 minutes

2. **Verify Storage Location**:
   - Settings → Recording → Output Folder
   - Ensure folder exists and is writable
   - Try changing to different drive

3. **Check Windows Game DVR**:
   - Open Xbox Game Bar Settings
   - Storage → Change storage location
   - Test recording with Xbox Game Bar directly

4. **Disable Background Applications**:
   - Close OBS, Streamlabs, or other recording software
   - Only one application can use Game DVR at a time

---

### "Can't find clips after recording"

**Symptoms**: Recording completed, but clips don't appear in gallery.

**Solutions**:

**1. Check Recording Was Successful**:
- ✅ Dashboard → Recent Games
- ✅ Look for game entry with video file path
- ✅ Navigate to file path and verify video exists

**2. Refresh Gallery**:
- ✅ Click "Refresh" button in gallery view
- ✅ Or restart LoLShorts

**3. Check Event Detection**:
- ✅ Clips are created based on detected events (kills, objectives)
- ✅ If game had no significant events, no clips created
- ✅ Expected: 3-10 clips per game (varies by performance)

**4. Database Sync**:
- ✅ Wait 30-60 seconds after game ends
- ✅ Background processing may still be running
- ✅ Check progress indicator

---

## Auto-Edit Issues

### "No clips found for the selected games"

**Error Message**:
```
No clips found for the selected games

Make sure you have:
- Recorded some games
- Interesting events occurred (kills, objectives, etc.)
- Clips were successfully saved
```

**Solutions**:

**1. Verify Games Have Clips**:
- ✅ Go to Gallery view
- ✅ Check selected games have associated clips
- ✅ If no clips, record more games first

**2. Check Event Detection Threshold**:
- ✅ Settings → Auto-Edit → Event Priority Threshold
- ✅ Lower threshold to include lower-priority clips
- ✅ Recommended: 2 (includes most kills and objectives)

**3. Re-Process Game**:
- ✅ Go to specific game in dashboard
- ✅ Click "Reprocess Events"
- ✅ Wait for event detection to complete
- ✅ Try Auto-Edit again

---

### "Not enough clips to create 60s video"

**Error Message**:
```
Not enough clips to create 60s video

Found: 25s of clips
Required: 60s

Try:
- Selecting more games
- Reducing target duration
- Lowering priority threshold
```

**Solutions**:

**Immediate Fix**:
- ✅ Select more games (add 1-2 additional games)
- ✅ OR reduce target duration to 30s or 45s
- ✅ OR lower priority threshold in settings

**Long-Term Fix**:
- ✅ Record more games with higher performance
- ✅ Focus on games with pentakills, baron steals, multi-kills
- ✅ Average game needs 3-5 clips minimum

---

### "Video generation failed: Failed to merge video clips"

**Cause**: Clip files have incompatible formats or are corrupted.

**Solutions**:

**1. Check Clip File Integrity**:
- ✅ Navigate to clips folder
- ✅ Try playing clips in VLC Media Player
- ✅ If clips won't play, they're corrupted (re-record)

**2. Clear Temp Files**:
```
1. Close LoLShorts and allow the five-second shutdown budget to finish
2. Restart LoLShorts and inspect the processing job's recovery status
3. Open Settings > Diagnostics if the job remains blocked
4. Preserve the diagnostic bundle and affected file; do not manually delete the
   whole app-data temp directory because it can contain recoverable job state
```

**3. Verify the bundled FFmpeg sidecar**:
- ✅ End users should install the latest signed LoLShorts build; do not replace
  bundled executables manually
- ✅ Developers should run `prepare_ffmpeg.ps1 -Source Auto` and then
  `npm run verify:release-contract`
- ✅ Restart LoLShorts and retain diagnostics if the executable check still fails

**4. Check System Resources**:
- ✅ Close other applications
- ✅ Ensure at least 2GB free RAM
- ✅ Check CPU isn't at 100% usage

---

### "Generation takes longer than 5 minutes"

**Expected Performance**: <30 seconds per minute of output video

**Solutions**:

**1. Reduce Clip Count**:
- ✅ Select fewer games (try 1-2 games)
- ✅ Reduce target duration (try 60s instead of 180s)

**2. Simplify Canvas**:
- ✅ Remove complex canvas overlays temporarily
- ✅ Use simple text-only overlays
- ✅ Disable background images

**3. Disable Audio Mixing**:
- ✅ Generate without background music first
- ✅ Test if audio mixing is causing delay

**4. Check System Performance**:
- ✅ Close browser, Discord, OBS, other heavy apps
- ✅ Check CPU temperature (overheating causes throttling)
- ✅ Verify antivirus isn't scanning video files

**5. Upgrade Hardware** (if consistently slow):
- FFmpeg is single-threaded and CPU-intensive
- Faster CPU = faster video generation
- SSD significantly faster than HDD for video I/O

---

## Canvas & Graphics Issues

### "Canvas elements not appearing in final video"

**Symptoms**: Canvas looks correct in editor, but elements missing in generated video.

**Solutions**:

**1. Check Element Positions**:
- ✅ Ensure X and Y are between 0-100 (percentage)
- ✅ Elements outside bounds won't render
- ✅ Use canvas editor preview to verify

**2. Verify File Paths**:
- ✅ For image elements, check file path is correct
- ✅ Use absolute paths (C:\...\image.png)
- ✅ Ensure files haven't been moved/deleted

**3. Check Text Contrast**:
- ✅ White text on white background is invisible
- ✅ Add black outline to all text elements
- ✅ Use high-contrast colors

**4. Re-Save Template**:
- ✅ Open canvas editor
- ✅ Re-apply all elements
- ✅ Save as new template
- ✅ Try generation again

---

### "Text is blurry or pixelated"

**Cause**: Font rendering issues or low resolution.

**Solutions**:

**1. Use System Fonts First**:
- ✅ Try Arial, Calibri, or Impact (guaranteed to work)
- ✅ Custom fonts may have rendering issues

**2. Increase Font Size**:
- ✅ Minimum 24px for readability
- ✅ Recommended: 32-48px for titles

**3. Check Output Resolution**:
- ✅ Videos are rendered at 1080x1920
- ✅ Canvas scales from 360x640 preview
- ✅ This is expected, not a bug

**4. Use TrueType Fonts (.ttf)**:
- ✅ OTF fonts may not render correctly
- ✅ Convert to TTF if needed

---

### "Logo/image looks stretched or distorted"

**Cause**: Image aspect ratio doesn't match element size.

**Solution**:

**1. Maintain Aspect Ratio**:
- ✅ If logo is 500x500 (1:1), use 60x60 canvas size
- ✅ If logo is 1000x500 (2:1), use 120x60 canvas size

**2. Pre-Resize Images**:
- ✅ Use Photoshop/GIMP to resize to exact dimensions
- ✅ Recommended: 300x300 for logos (1:1 ratio)
- ✅ Export as PNG with transparency

**3. Use Correct Format**:
- ✅ PNG for logos (supports transparency)
- ✅ JPG for photos (no transparency)
- ✅ Avoid BMP, TIFF, or other formats

---

## Audio Issues

### "Background music is too loud / too quiet"

**Solution**: Adjust audio mixer levels

**Quick Fix**:
```
1. Go to Auto-Edit → Audio tab
2. Try different presets:
   - Music too loud? → Game Focus (85% / 15%)
   - Music too quiet? → Cinematic (60% / 40%)
3. Generate test video to verify
```

**Manual Adjustment**:
```
1. Click "Custom" preset
2. Adjust sliders:
   - Game Audio: 60-90%
   - Background Music: 10-40%
3. Test different values in 10% increments
```

**See**: [Audio Mixing Best Practices](./AUDIO_MIXING.md) for detailed guide.

---

### "Audio is out of sync with video"

**Symptoms**: Audio plays before/after video action, lip sync issues.

**Solutions**:

**1. This Should Not Happen**:
- ✅ LoLShorts automatically syncs audio/video
- ✅ If occurring, this is a bug - please report!

**2. Check Source Video**:
- ✅ Play original recording in VLC
- ✅ If source is out of sync, re-record

**3. Workaround**:
- ✅ Export video to editor (Premiere Pro)
- ✅ Manually adjust audio offset
- ✅ Re-export final video

---

### "Music doesn't play / silent background music"

**Causes & Solutions**:

**1. Check File Format**:
- ✅ Supported: MP3, WAV, M4A, AAC
- ✅ Not supported: FLAC, OGG, WMA
- ✅ Convert unsupported formats to MP3

**2. Check File Isn't Corrupted**:
- ✅ Play music file in Windows Media Player
- ✅ If won't play, file is corrupted
- ✅ Re-download music file

**3. Check Background Music Volume**:
- ✅ Ensure background music slider isn't at 0%
- ✅ Minimum 15% to hear music

**4. Check "Enable Background Music" Toggle**:
- ✅ Audio tab → Ensure toggle is ON
- ✅ Music file path is shown

---

## Performance Issues

### "Application is slow / laggy"

**Symptoms**: UI stutters, menus take long to load, general sluggishness.

**Solutions**:

**1. Close Background Applications**:
- ✅ Close browser (Chrome uses lots of RAM)
- ✅ Close Discord, Spotify, OBS
- ✅ Check Task Manager for high CPU/RAM usage

**2. Reduce Gallery Size**:
- ✅ Delete old games/clips from database
- ✅ Archive old videos to external drive
- ✅ Settings → Storage → "Clean Up Old Files"

**3. Check System Resources**:
- ✅ 4GB RAM minimum, 8GB recommended
- ✅ SSD significantly faster than HDD
- ✅ Ensure Windows isn't updating

**4. Disable Animations** (if very slow):
- ✅ Settings → UI → Disable animations
- ✅ Reduces visual effects for performance

---

### "High CPU usage / computer becomes hot"

**Expected Behavior**: During video generation, CPU usage 70-90% is normal.

**Not Normal**: CPU at 100% when idle or browsing gallery.

**Solutions**:

**If High Usage During Generation**:
- ✅ This is expected (FFmpeg is CPU-intensive)
- ✅ Close other apps during generation
- ✅ Ensure good laptop cooling/ventilation

**If High Usage When Idle**:
- ✅ Check Task Manager → LoLShorts process
- ✅ Restart application
- ✅ Report as bug if persists

---

### "Disk space running out quickly"

**Causes**:
- Raw recordings: ~1-2GB per 30min game
- Clips: ~50-100MB per 10s clip
- Generated videos: ~20-30MB per 60s video

**Solutions**:

**1. Change Recording Location**:
- ✅ Settings → Recording → Output Folder
- ✅ Choose drive with more space

**2. Auto-Delete Old Recordings**:
- ✅ Settings → Storage → "Auto-delete recordings after X days"
- ✅ Recommended: 30 days
- ✅ Keeps clips, deletes full recordings

**3. Manually Clean Up**:
- ✅ Go to Gallery
- ✅ Select old games → Delete
- ✅ Or manually delete files from Windows Explorer

**4. Archive to External Drive**:
- ✅ Move old videos to external HDD/SSD
- ✅ Keep database entries, delete local files

---

## Export & File Issues

### "Can't find generated video file"

**Default Location**: `C:\Users\[You]\Videos\LoLShorts\auto_edit\`

**Solutions**:

**1. Check Output Location**:
- ✅ Settings → Auto-Edit → Output Folder
- ✅ Click "Open Folder" button
- ✅ Verify folder isn't empty

**2. Check File Name**:
- ✅ Format: `autoedit_[YYYY-MM-DD]_[HHMMSS].mp4`
- ✅ Example: `autoedit_2025-01-06_143022.mp4`
- ✅ Sort by "Date Modified" (newest first)

**3. Check Generation Succeeded**:
- ✅ Auto-Edit page should show "Complete" status
- ✅ If failed, error message explains reason
- ✅ Check logs: `C:\Users\[You]\AppData\Local\LoLShorts\logs\`

---

### "Video won't play / codec error"

**Symptoms**: Generated video won't play in Windows Media Player or browsers.

**Solutions**:

**1. Use VLC Media Player**:
- ✅ Download VLC (free): [videolan.org](https://www.videolan.org/)
- ✅ VLC plays all formats, including LoLShorts videos
- ✅ Set VLC as default media player

**2. Install Codecs**:
- ✅ Download K-Lite Codec Pack: [codecguide.com](https://codecguide.com/)
- ✅ Install "Basic" version
- ✅ Videos should now play in all players

**3. Check Video Isn't Corrupted**:
- ✅ File size should be >10MB for 60s video
- ✅ If <1MB, video is corrupted
- ✅ Re-generate video

---

### "Video quality is poor / looks pixelated"

**Symptoms**: Video looks blurry, pixelated, or low quality compared to original recordings.

**Solutions**:

**1. Check Source Video Quality**:
- ✅ Original recordings quality determines output quality
- ✅ Settings → Recording → Increase bitrate/resolution
- ✅ Re-record games in higher quality

**2. Understand Compression**:
- ✅ LoLShorts uses CRF 23 (good quality balance)
- ✅ Some compression is expected for file size
- ✅ This is YouTube Shorts standard

**3. Adjust Export Settings** (v1.3.0+):
- ✅ Settings → Export → Video Quality
- ✅ Increase quality (CRF 18-20 for higher quality)
- ✅ Note: Larger file sizes

**4. Check Device/Platform**:
- ✅ Quality may appear lower on mobile vs desktop
- ✅ Review the exported file on your target platform or device to check final quality
- ✅ Platform compression affects final quality

---

## League Client (LCU) Issues

### "Can't connect to League Client"

**Symptoms**: LCU status shows "Disconnected" or "Error".

**Solutions**:

**1. Ensure League is Running**:
- ✅ Launch League of Legends client FIRST
- ✅ Wait until you can see friends list
- ✅ Then start LoLShorts

**2. Restart Both Applications**:
```
1. Close LoLShorts
2. Close League Client (exit completely)
3. Start League Client
4. Wait 30 seconds
5. Start LoLShorts
6. Check LCU status
```

**3. Check Firewall**:
- ✅ Windows Security → Firewall
- ✅ Allow LoLShorts through firewall
- ✅ Allow League Client through firewall

**4. Run as Administrator**:
- ✅ Right-click LoLShorts → "Run as administrator"
- ✅ Needed for LCU API access

---

### "LCU connection works but events not detected"

**Symptoms**: Connected to League but kills/objectives don't trigger event detection.

**Solutions**:

**1. Check Game Mode**:
- ✅ Event detection works in: Summoner's Rift (ranked, normal, draft)
- ✅ Limited support: ARAM, Nexus Blitz
- ✅ Not supported: Tutorial, Practice Tool, Custom Games (vs. Bots)

**2. Enable In-Game Events**:
- ✅ In-game: Press ESC → Interface → Enable all event notifications
- ✅ LoLShorts reads from game events API

**3. Check API Permissions**:
- ✅ League Client settings → allow third-party integrations
- ✅ Restart both applications after enabling

---

## Error Messages Reference

### "Output directory not found: [path]"

**Solution**:
```
1. Check Settings → Auto-Edit → Output Folder
2. Ensure folder exists on your system
3. Create folder if missing: Right-click → New → Folder
4. Or change to existing folder (e.g., Desktop)
```

---

### "Video file is corrupted or invalid"

**Solutions**:
- ✅ Re-record the affected game
- ✅ Check if video plays in VLC Player
- ✅ If source recording is corrupted, cannot be recovered
- ✅ Ensure sufficient disk space during recording

---

### "System resources exhausted"

**Cause**: Insufficient RAM or CPU for video processing.

**Solutions**:
- ✅ Close all other applications
- ✅ Reduce clip count (select fewer games)
- ✅ Restart computer to free up RAM
- ✅ Upgrade RAM (8GB recommended minimum)

---

### "Video processing timeout (operation took longer than 300s)"

**Cause**: Operation exceeded 5-minute timeout.

**Solutions**:
- ✅ Reduce target duration (180s → 60s)
- ✅ Select fewer games
- ✅ Disable canvas overlay and audio mixing
- ✅ Check CPU usage in Task Manager
- ✅ Close background applications

---

## Getting Help

### Before Asking for Help

**Gather Information**:
1. What were you trying to do?
2. What error message did you see? (screenshot helps)
3. What steps did you already try?
4. Check logs: `C:\Users\[You]\AppData\Local\LoLShorts\logs\app.log`

---

### Support Channels

**GitHub Issues** (bugs and feature requests):
- [github.com/yourusername/lolshorts/issues](https://github.com/yourusername/lolshorts/issues)
- Best for: Bug reports, feature requests
- Response time: 1-3 days

**Discord Community** (quick help):
- [discord.gg/lolshorts](https://discord.gg/lolshorts)
- Best for: Quick questions, troubleshooting
- Response time: <1 hour (community-driven)

**Email Support** (account/payment issues):
- support@lolshorts.com
- Best for: License issues, billing, account problems
- Response time: 1-2 business days

---

### Reporting Bugs

**Good Bug Report Template**:
```markdown
**Description**: Short description of the issue

**Steps to Reproduce**:
1. Go to Auto-Edit page
2. Select 2 games
3. Click "Generate Video"
4. Error appears: [error message]

**Expected Behavior**: Video should generate successfully

**Actual Behavior**: Error message appears, video not created

**System Info**:
- OS: Windows 11 Pro
- LoLShorts Version: 1.2.0
- League Client Version: 14.1

**Logs** (attach app.log from %LOCALAPPDATA%\LoLShorts\logs\)
```

---

## Still Having Issues?

If troubleshooting doesn't resolve your problem:

1. **Search existing issues**: [GitHub Issues](https://github.com/yourusername/lolshorts/issues)
2. **Ask on Discord**: Often fastest solution
3. **File a bug report**: Include logs and detailed steps
4. **Email support**: For account-specific issues

**We want to help!** Please provide as much information as possible to help us resolve your issue quickly.

---

**Version**: 1.2.0
**Last Updated**: 2025-01-06
**Back to**: [Auto-Edit Guide](./AUTO_EDIT_GUIDE.md)
