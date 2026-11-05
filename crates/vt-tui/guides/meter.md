# The meter

The column on the right of the screen shows the main output, after the
limiter. It is what you hear. It hides on a very narrow terminal.

```
Main            the title
  -3  2         highest recent peak in dB, and the most recently limited
L  R  G         left, right, and how much the limiter is taking off
```

## The L and R bars

| Part | Means |
|---|---|
| Solid colour | The average level, close to how loud it sounds |
| Dimmer colour above it | The peak, which jumps with each hit |
| A line above that | The highest recent peak, held for a moment |

The numbers are dB below full level. The top of the scale is spread out, as
that is where the detail matters. The colour goes from green to yellow near
`-18`, to orange near `-9`, and to red in the last `3`.

## The G bar

G is gain reduction: how many dB the limiter is taking off right now. It
hangs down from the top, and its full height is `12`. It has its own scale,
so the numbers beside L and R don't apply to it.

The limiter is a soft curve with no hard edge, so it is always taking off a
very little, and G is rarely exactly zero:

| Peak on L and R | G |
|---|---|
| `-12` | Under `1`, and hard to hear |
| `-6` | Still under `1` |
| `-3` | About `2`, and the top number turns red |

So a red number above G means the limiter is doing real work. That may be
what you want on a loud hit. If it is red all the time and things sound
squashed, turn some tracks down with `:tvol`.
