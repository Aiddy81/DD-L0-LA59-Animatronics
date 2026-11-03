# Get the simulator

> **You'll need:** a Windows, Linux or Mac computer (see the table below) · **Time:** about 10 minutes

The simulator comes as one download with everything inside: the program, its own Python and Lola's code. There's nothing else to install.

| Your computer | Download the file that starts with |
|---|---|
| Windows 10 or 11 | `ddl0-sim-windows-` … `.zip` |
| Linux (64-bit PC) | `ddl0-sim-linux-` … `.tar.gz` |
| Mac with an Apple chip (M1 or newer, macOS 11 or later) | `ddl0-sim-macos-arm64-` … `.tar.gz` |

Older Macs with an Intel chip can't run it.

## On Windows

1. Go to the [latest release](https://github.com/Aiddy81/DD-L0-LA59-Animatronics/releases/latest).
2. Under **Assets**, click **`ddl0-sim-windows-…zip`** to download it.
3. Open your **Downloads** folder.
4. Right-click the ZIP file and choose **Extract All…**, then click **Extract**.
5. In the new folder, open **`simulator`**, then **`dist`**, and double-click **`ddl0-sim.exe`**.
6. If Windows shows "Windows protected your PC", click **More info**, then **Run anyway**. This appears because the program is new and not signed by a big company.

## On a Mac

1. Go to the [latest release](https://github.com/Aiddy81/DD-L0-LA59-Animatronics/releases/latest).
2. Under **Assets**, click **`ddl0-sim-macos-arm64-…tar.gz`** to download it.
3. Double-click the downloaded file in **Finder** to unpack it.
4. In the new folder, open **`simulator`**, then **`dist`**.
5. Right-click **`ddl0-sim`**, choose **Open**, then **Open** again. On macOS 15 or later: try to open it once, then go to **System Settings > Privacy & Security** and click **Open Anyway**.

The Mac asks this the first time because the app isn't signed by Apple.

## On Linux

1. Go to the [latest release](https://github.com/Aiddy81/DD-L0-LA59-Animatronics/releases/latest) and download **`ddl0-sim-linux-…tar.gz`**.
2. Open a terminal in your downloads folder and unpack it (put the version you downloaded in place of `<version>`):

   ```bash
   tar -xzf ddl0-sim-linux-<version>.tar.gz
   ```

3. Start it:

   ```bash
   ./ddl0-sim-linux-<version>/simulator/dist/ddl0-sim
   ```

It needs a Vulkan graphics driver (Mesa's `mesa-vulkan-drivers` on Debian and Ubuntu) and a normal desktop (X11 or Wayland). Most desktop installs already have everything.

### ✅ It should look like this

After a few seconds Lola appears and starts her start-up wiggle, then breathes slowly.

![The simulator window just after starting: the 3D view of Lola in the middle, the toolbar along the top and panels around the sides](images/get-first-start.png)

## Choose the firmware folder

The simulator needs the robot's code (the folder with `code.py` in it) to know what Lola should do.

- The download already has it, so the simulator finds it by itself.
- If it can't, it asks you to pick a folder. Choose the **`PropMaker_Feather`** folder.

To switch to a different copy later, click **🗁 Firmware** at the right of the toolbar.

![The 🗁 Firmware button at the right end of the toolbar, circled](images/get-firmware-button.png)

## Add her sounds

The sound files aren't in the download. Without them Lola still moves and lights up, but stays quiet. To hear her, put WAV files in `PropMaker_Feather/sounds/` with the names on the [Sound list](Sound-List). See [Adding sounds](Adding-Sounds).

## ⚠️ Something went wrong?

| What you see | Try this |
|---|---|
| Nothing happens when you double-click (Windows) | Wait 10 seconds; the first start is slow. Check you extracted the ZIP: running the exe from inside the ZIP doesn't work. |
| "Windows protected your PC" with no *Run anyway* | Click **More info** first. If your computer is managed by a school or work, it may block new programs; ask whoever looks after it. |
| Mac says it "can't be opened" or "is damaged" | That's Gatekeeper. Use **right-click > Open**, or **Open Anyway** in *Privacy & Security* (step 5). |
| Linux: nothing opens, or an error about Vulkan or Wayland | Install your graphics card's Vulkan driver, and check `libwayland-client`, `libxkbcommon` and `libasound2` are installed. |
| It asks for a folder | Pick the `PropMaker_Feather` folder inside the download. |
| Lola looks simpler than in the pictures | There are two 3D models: **Lola** (detailed) and **Primitive** (simpler and faster). Pick one under **Model** in the toolbar's **Options**. |

More fixes: [Troubleshooting and FAQ](Simulator-Troubleshooting).

## More detail (for tinkerers)

- **Check the download:** each release lists every file's SHA-256 (also in `SHA256SUMS`). Compare it with yours:

  ```powershell
  Get-FileHash .\ddl0-sim-windows-<version>.zip -Algorithm SHA256
  ```

  On Linux use `sha256sum`, on a Mac `shasum -a 256`.
- **Mac, from the terminal:** instead of right-click > Open, clear the quarantine flag on the unpacked folder: `xattr -dr com.apple.quarantine ddl0-sim-macos-arm64-<version>`.
- **Linux limits:** a panel torn off into its own window opens at the default position, and *Follow system* reads GNOME's settings only. The **🗁 Firmware** dialog needs an XDG desktop portal.
- **Newer than the last release:** every CI run attaches a build (`ddl0-sim-windows`, `ddl0-sim-linux`, `ddl0-sim-macos-arm64`) for 14 days. Open the [Simulator runs](https://github.com/Aiddy81/DD-L0-LA59-Animatronics/actions/workflows/simulator.yml), pick a green run, and download it at the bottom (you need to be signed in). It's only the `dist` folder, so put it at `simulator/dist/` in a copy of the project. On Linux and Mac, also run `chmod +x ddl0-sim`.
- The simulator picks its firmware in this order: a `code.py` path given as the first command-line argument; the folder last chosen with **🗁 Firmware** (if it still has `code.py`); `PropMaker_Feather/code.py` in a folder above the program. If none is found it asks.
- Its own files (3D model, `sim.toml`, `parts.toml`, scenarios) come from the `simulator` folder above `dist`, so the firmware can live anywhere: a copy, another checkout, or even the CIRCUITPY drive.
- Your settings are kept in `ddl0-sim.prefs.toml` next to the program. Delete it to go back to the defaults.
- To build it yourself, see [Building the simulator](Building-the-Simulator).
