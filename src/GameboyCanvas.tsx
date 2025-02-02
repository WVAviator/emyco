import { onCleanup, onMount } from 'solid-js';
import { listen } from '@tauri-apps/api/event';

const WIDTH = 160;
const HEIGHT = 144;

const PALETTE = [
  [155, 188, 15, 255],
  [139, 172, 15, 255],
  [48, 98, 48, 255],
  [15, 56, 15, 255],
];

const GameboyCanvas = () => {
  let canvas: HTMLCanvasElement | undefined;
  let ctx: CanvasRenderingContext2D | null = null;
  let imageData: ImageData;

  onMount(() => {
    if (!canvas) return;
    ctx = canvas.getContext('2d');
    if (!ctx) return;

    imageData = ctx.createImageData(WIDTH, HEIGHT);

    const unlisten = listen<Uint8Array>('gb-present-frame', (event) => {
      const frame = new Uint8Array(event.payload);

      for (let i = 0; i < frame.length; i++) {
        const [r, g, b, a] = PALETTE[frame[i]];
        imageData.data.set([r, g, b, a], i * 4);
      }

      ctx?.putImageData(imageData, 0, 0);
    });

    onCleanup(() => {
      unlisten.then((f) => f());
    });
  });

  return (
    <canvas
      ref={canvas}
      width={WIDTH}
      height={HEIGHT}
      class="w-full h-full"
      style={{
        'image-rendering': 'pixelated',
      }}
    />
  );
};

export default GameboyCanvas;
