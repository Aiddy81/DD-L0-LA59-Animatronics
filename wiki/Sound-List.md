# Sound list

All in `PropMaker_Feather/sounds/`. File names are case-sensitive.

| File | Played by | When |
|---|---|---|
| `noseBoops.wav` | `nose_boops()` | A boop |
| `angry.wav` | `angry_animation()` | Angry |
| `youWho.wav` | `attention_animation()` | Wanting attention after being left alone |
| `trenchRun.wav` | `trench_run()` | The trench run (a short clip, looped for the whole run) |
| `001.wav`, `003.wav`, `005.wav`, `007.wav`, `008.wav` | `code.py` | Extra sounds for the start-up pick |

**Start-up sound:** once, just after the start-up wiggle, `code.py` plays one file picked at random from **every** WAV in `sounds/`, including the named ones above. Add a WAV to `sounds/` and it joins the draw.

Every file must be **22050 Hz, stereo, 16-bit signed PCM**. See [Adding sounds](Adding-Sounds).
