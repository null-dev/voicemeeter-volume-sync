# voicemeeter-volume-sync

This program syncs the volume from Windows to VoiceMeeter.

It is similar to: https://github.com/Frosthaven/voicemeeter-windows-volume but much more lightweight.
In fact, it performs exactly zero work in the background and sleeps most of the time.
It only wakes up when you change the volume and goes right back to sleep afterwards. Memory usage never goes higher than 5 MB.

## Usage

Currently the behavior is hardcoded as I wrote this mainly for my own setup.
See the `update_volume` method for more information.

## License
Apache 2.0