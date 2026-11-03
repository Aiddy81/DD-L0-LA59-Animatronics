# Troubleshooting and FAQ

Find your problem in the list. If it isn't here, [open an issue](https://github.com/Aiddy81/DD-L0-LA59-Animatronics/issues/new) and say what you did and what you saw.

## Starting the app

| Problem | Fix |
|---|---|
| Nothing happens when I double-click `ddl0-sim.exe` | Extract the ZIP first (right-click > **Extract All…**). The exe needs the files next to it. |
| "Windows protected your PC" | Click **More info**, then **Run anyway**. |
| Mac: "can't be opened" or "is damaged" | Right-click `ddl0-sim` > **Open**, or **System Settings > Privacy & Security > Open Anyway**. |
| Linux: it won't start | It needs a Vulkan driver and `libwayland-client`, `libxkbcommon` and `libasound2`. See [Get the simulator](Get-the-Simulator#on-linux). |
| It asks me to pick a folder | It can't find the firmware. Pick the `PropMaker_Feather` folder from your copy of the project. |
| A box says something went wrong at start-up | Read the message: it names the missing file. Usually the `simulator` folder isn't above the exe, or the download is incomplete. Download it again. |

## While it's running

| Problem | Fix |
|---|---|
| Lola doesn't move at all | Look at the status bar. If it says *stopped*, click **▶ Run**. If *paused*, press **Space**. If the **Log** shows a red error, see the next row. |
| The **Log** shows a red traceback | The firmware has an error. If you changed a file, undo your last change. Click the blue line to open the file at the problem. |
| Boops don't work | Click without moving the mouse (a drag turns the camera). If **Edit parts** is on in the **Parts** panel, turn it off. |
| I can't make her angry | See [Make her angry](Droid-Behaviours#3-make-her-angry). **Hard shake** always works. |
| No sound | The sounds aren't included: [add your own](Adding-Sounds). Then check **Mute** in the **Audio** panel and your computer's volume. |
| The app is slow or jerky | Close other big programs. Pick **Model > Primitive** in the toolbar's *Options* for a simpler 3D model. |
| A panel disappeared | **⚙ View** and tick it, or **⚙ View > Layouts > Reset layout**. |
| Keys don't do anything | You're typing in a text box. Click the 3D view or press **Esc** first. |

## Questions

**Is the simulator exactly like the robot?**
Very close. It runs the same code, and copies the real board's speed, memory, servo speed and sensor limits. Real servos and LEDs can vary a little, and the 3D body isn't a perfect copy.

**Can I break the robot by playing with the simulator?**
No. The simulator never touches the robot unless you press **⬆ Deploy** with the robot plugged in.

**Does it change my files?**
Only when you press **Save** in the **Settings (settings.toml)** or **Parts** panels. Anything the firmware writes, such as `log.txt`, goes to the `fs` folder next to the exe, not into your project.

**Where are my screenshots?**
In the `captures` folder next to `ddl0-sim.exe`.

**How do I go back to all the default settings?**
Close the app and delete `ddl0-sim.prefs.toml` next to the exe.

**Does it run on Mac or Linux?**
Yes: 64-bit Linux and Macs with an Apple chip. See [Get the simulator](Get-the-Simulator). Intel Macs aren't supported.

**Why doesn't she make any sound?**
The sound files aren't included in the project or the download. Add your own: [Adding sounds](Adding-Sounds).

## More detail (for tinkerers)

- **`[sim] … wing servo can't keep up`** in the log means the firmware asks the wing to move faster than a real FS90 can. On the robot that animation will look slower or fall short.
- **`[sim] <task> held the loop for N ms without an await`** means a behaviour blocked every other one, often a `time.sleep()` that should be `await asyncio.sleep()`.
- **Heap warnings at 80% / 100%** mean the code may not fit in the board's memory.
- **`ImportError` with CircuitPython strict on** means you used something CircuitPython 9 doesn't have. Untick **CircuitPython strict** to check, but the robot will fail the same way.
- A module listed under *not simulated* in the **Board** panel is a stand-in that does nothing. Write a [shim](Adding-New-Hardware) to make it work.
