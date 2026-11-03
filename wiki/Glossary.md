# Glossary

Words used in this wiki, in plain language. In alphabetical order.

| Word | What it means |
|---|---|
| **Accelerometer** | A tiny sensor that feels movement and which way is down. Lola's is called the **LIS3DH**. It's how she knows she's been booped. |
| **asyncio** | A part of Python that lets one program do several things "at once" by taking turns quickly. Lola breathes, listens for boops and waits for attention all at the same time this way. |
| **Behaviour** | One thing Lola does, like breathing or getting angry. Each one is its own file in `behaviours/`. |
| **Boop** | A gentle tap on Lola. She answers with a happy wing flutter. |
| **CIRCUITPY** | The name of the USB drive that appears on your computer when you plug in the robot's board. The firmware files live on it. |
| **CircuitPython** | A small version of the Python programming language that runs on little boards like Lola's. She uses version 9. |
| **Docker** | A program that builds software inside a sealed box (a "container"), so nothing extra gets installed on your computer. Only needed to *build* the simulator, not to use it. |
| **Duty (duty cycle)** | How much of the time a light is switched on, very quickly on and off. More duty means brighter. |
| **Feather** | Adafruit's family of small boards. Lola's brain is the **RP2040 Prop-Maker Feather**. |
| **FeatherWing** | An add-on board that stacks on a Feather. Lola uses the **8-Channel PWM/Servo FeatherWing** to drive her wings and eyes. |
| **Firmware** | The program that runs on the robot. For Lola that's `code.py` and the files next to it. |
| **g** | A unit of how hard something is pushed. 1 g is gravity, which the accelerometer always feels when Lola sits still. A boop is a bit more; a shake is a lot more. |
| **LED** | A small light. Lola has two colour (**RGB**) LEDs for eyes and one white LED. |
| **Log** | A list of messages the firmware writes as it runs, such as "Boot sequence complete!". Useful when something goes wrong. |
| **mg (milli-g)** | A thousandth of a g. Settings like `NUDGE_Z_MG = 250` mean 0.25 g. |
| **Nudge** | A small push, big enough to count as a boop. |
| **Panel** | One box in the simulator's window, such as *Motion* or *Log*. You can move, hide and float panels. |
| **PCA9685** | The chip on the Servo FeatherWing. It has 16 outputs ("channels"); Lola uses 8 for her wings and eyes. See the [Channel map](Channel-Map). |
| **PWM** | Pulse-width modulation: switching something on and off very fast. The length of each "on" pulse sets a servo's position or a light's brightness. |
| **RGB** | Red, green and blue. Mixing these three makes any colour. |
| **Scenario** | A script for the simulator that pokes Lola at set times and checks she reacts correctly. |
| **Servo** | A small motor that turns to an exact angle when told. Lola has two, one per wing. |
| **settings.toml** | A text file of numbers you can change, like how long she waits before asking for attention. See [Settings reference](Settings-Reference). |
| **Shim** | A stand-in. The simulator swaps the robot's hardware libraries for shims that move the 3D model instead of real motors. |
| **Simulator** | The Windows program that shows Lola in 3D and runs her real firmware, so you can play without the robot. |
| **Trench run** | A 2½-minute "flight" Lola sometimes does when she wants attention: her wings tilt and dive, and her eyes glow orange. |
| **WAV** | A sound file. Lola's sounds are in the `sounds/` folder. |
