# Tickeys
![Tickeys Icon](https://raw.githubusercontent.com/yingDev/Tickeys/master/.readme_images/icon.png)

<img src="https://raw.githubusercontent.com/yingDev/Tickeys/master/.readme_images/Tickeys%20new.png" width="128" height="128" />
<br>(An alternative icon designed by [@WillStark](https://github.com/WillStark) )

Instant audio feedback for typing. For macOS. 

A demo for learning [Rust](https://www.rust-lang.org).

### Other versions:
- Linux: [Tickeys-linux](https://github.com/BillBillBillBill/Tickeys-linux)
- Windows: [Download](https://www.yingdev.com/Content/Projects/Tickeys_Win/Release/1.1.1/Tickeys1.1.1.rar)

# Install

## Build from source (this fork, 1.0.0+)

Requirements: macOS, Rust (`brew install rust` or rustup). No Xcode needed.

```sh
# 1. one-time: create the local code-signing identity "Tickeys Local"
scripts/setup-signing.sh

# 2. build + assemble + sign  ->  build/Tickeys.app
scripts/build.sh

# 3. (optional) install straight to /Applications
scripts/build.sh --install
```

Then launch `Tickeys.app`:

1. A dialog asks you to enable Tickeys in
   **System Settings → Privacy & Security → Accessibility**.
   Tick it once, come back, click "继续".
2. Done. The menu-bar icon appears; type anywhere to hear keys;
   press `QAZ123` (or click the icon → Open Settings) for settings.

### Why the signing dance?

macOS ties the Accessibility authorization to the app's code signature.
Unsigned or ad-hoc-signed apps get a *new* identity on every rebuild, so the
permission grant is lost after each upgrade — the well-known
"keeps asking for permission again" loop. Signing every build with the same
local certificate ("Tickeys Local") gives the app a stable identity:
you grant the permission **once**, and future rebuilds/upgrades never ask again.

> Upgrading from the old ad-hoc builds (`com.yingDev.Tickeys.SwitchPacks`)?
> Remove their stale permission entry with:
> `tccutil reset Accessibility com.yingDev.Tickeys.SwitchPacks`

## Original upstream install

  - brew cask
```sh
brew cask install tickeys && open /Applications/Tickeys.app
```
  - or download the [dmg](https://github.com/yingDev/Tickeys/releases/download/0.5.0/Tickeys-0.5.0-yosemite.dmg)

# Screenshots

<img src="https://raw.githubusercontent.com/yingDev/Tickeys/master/.readme_images/1.png" alt='sound effects' width=400/>
<br/>
<img src="https://raw.githubusercontent.com/yingDev/Tickeys/master/.readme_images/2.png" alt='black/white list' width=400/>
<br/><br/>
<a href='https://www.youtube.com/watch?v=XeqA-LU5IWg' target='_blank'>
<img src="https://raw.githubusercontent.com/yingDev/Tickeys/master/.readme_images/video_thumb.png" alt='sound effects' width=400/>
</a>

# Add custom schemes
0. locate the `data` directory in Finder: `Tickeys.app/Content/Resources/data/`

1. copy & paste an effect directory and rename the copy, eg.`drum` -> `myDrum`

2. open `schemes.json` and edit it by copy & paste the corresponding scheme entry; change the `name`  and `display_name` as needed. eg:
	```json 
	,{
		"name": "myDrum",
		"display_name": "My Drum",
		"files": ["1.wav", "2.wav", "3.wav", "4.wav", "space.wav", "backspace.wav", "enter.wav"],
		"non_unique_count": 4, 
		"key_audio_map":{"36": 6, "49": 4, "51": 5}
	},
	```
	- note:
	 	* "name": value must be the same as your directory name
	 	* "files": sound file list 
	 	* "non_unique_count": first N items in `files` are auto mapped to keys
	  	* "key_audio_map": mappings of keyCode to sound index in "files". eg. 36 == `enter`
	 
3. add/replace your `.wav` files; update & save the json file

4. re-launch Tickeys. ("qaz123")

## Deps （for development)
* alut
* openssl
```sh
brew install freealut openssl
```

## License
* MIT
