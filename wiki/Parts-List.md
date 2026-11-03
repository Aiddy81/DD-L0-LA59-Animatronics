# Parts list

> **You'll need:** about AU$100–150 for the electronics, plus the body · Read [Safety first](Hardware-Safety)

## The electronics

| Part | How many | What it does | Where to get it |
|---|---|---|---|
| **Adafruit RP2040 Prop-Maker Feather** | 1 | Lola's brain. It has the motion sensor (accelerometer), a speaker amplifier and a battery charger built in. | [Adafruit 5768](https://www.adafruit.com/product/5768) |
| **Adafruit 8-Channel PWM or Servo FeatherWing** | 1 | Stacks on the Feather and drives the two wings and both eyes. | [Adafruit 2928](https://www.adafruit.com/product/2928) |
| **FEETECH FS90 micro servo** (9 g, 180°, 1.5 kg·cm) | 2 | Move the wings. | Any hobby electronics shop |
| **RGB LED, common cathode** | 2 | The main and small eyes. | Any electronics shop |
| **White LED** | 1 | The "thinking" light. | Any electronics shop |
| **Small speaker** (4–8 Ω) | 1 | For her sounds. Connects to the Feather's speaker terminals. | Any electronics shop |
| **LiPo battery** (3.7 V, JST-PH plug) | 1 | So she can run without a cable. | Adafruit, or a hobby shop |
| **USB-C data cable** | 1 | To put the firmware on her. | |
| Wire, header pins, heat-shrink | some | Joining it all up. | |

<!-- TODO(owner): add a photo of every part laid out, as images/parts-all.jpg -->
![All the electronic parts laid out on a table, labelled (photo to come)](images/TODO-parts-all.png)

## Tools

- Soldering iron and solder
- Wire cutters and strippers
- Small screwdriver for the screw terminals
- A computer with a USB port

## The body

Lola's body and wings are custom-made. Each wing is a panel turned by a servo on its hinge.

<!-- TODO(owner): link to the body files (3D-print STLs or plans) and add a photo of the finished droid as images/parts-finished-droid.jpg -->
![The finished DD-L0-LA59 droid (photo to come)](images/TODO-parts-finished-droid.png)

## ✅ Ready?

Got everything? Next: [Wiring](Wiring).

## More detail (for tinkerers)

- **Why common cathode?** The firmware makes an eye brighter by raising its PWM duty (more "on" time). That suits LEDs whose shared leg goes to ground (common cathode). Common-anode LEDs would light the wrong way round.
- **Why that servo?** The simulator models the FS90 at 4.8 V (0.12 s per 60°, `sec_per_60deg` in `simulator/sim.toml`). A different servo works, but you may need to recalibrate the wings and update `sim.toml`.
- **Different parts?** The firmware expects the PCA9685 at its default I²C address on the same bus as the accelerometer, and the white LED on the Feather's `EXTERNAL_NEOPIXELS` pin. See the [Channel map](Channel-Map).
