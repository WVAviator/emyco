import { invoke } from '@tauri-apps/api/core';
import GameboyCanvas from './GameboyCanvas';
import { useKeyPress } from '../hooks/useKeyPress';

const Gameboy = () => {
  useKeyPress({
    Escape: {
      keydown: () => {
        console.log('Sending start key down.');
        invoke('register_input', { key: 'start', down: true });
      },
      keyup: () => {
        console.log('Sending start key up.');
        invoke('register_input', { key: 'start', down: false });
      },
    },
    Tab: {
      keydown: () => {
        invoke('register_input', { key: 'select', down: true });
      },
      keyup: () => {
        invoke('register_input', { key: 'select', down: false });
      },
    },
    Enter: {
      keydown: () => {
        invoke('register_input', { key: 'a', down: true });
      },
      keyup: () => {
        invoke('register_input', { key: 'a', down: false });
      },
    },
    Backspace: {
      keydown: () => {
        invoke('register_input', { key: 'b', down: true });
      },
      keyup: () => {
        invoke('register_input', { key: 'b', down: false });
      },
    },
    w: {
      keydown: () => {
        invoke('register_input', { key: 'up', down: true });
      },
      keyup: () => {
        invoke('register_input', { key: 'up', down: false });
      },
    },
    s: {
      keydown: () => {
        invoke('register_input', { key: 'down', down: true });
      },
      keyup: () => {
        invoke('register_input', { key: 'down', down: false });
      },
    },
    a: {
      keydown: () => {
        invoke('register_input', { key: 'left', down: true });
      },
      keyup: () => {
        invoke('register_input', { key: 'left', down: false });
      },
    },
    d: {
      keydown: () => {
        invoke('register_input', { key: 'right', down: true });
      },
      keyup: () => {
        invoke('register_input', { key: 'right', down: false });
      },
    },
  });

  return (
    <section>
      <div class="flex gap-4 items-center justify-center w-full">
        <button
          class="cursor-pointer p-4"
          on:click={() => {
            invoke('start_emulator');
          }}
        >
          Start
        </button>
        <button
          class="cursor-pointer p-4"
          on:click={() => {
            invoke('stop_emulator');
          }}
        >
          Stop
        </button>
      </div>
      <div class="w-full relative z-0">
        <div class="relative w-full">
          <img
            class="absolute object-cover w-full scale-150 top-28 pointer-events-none"
            src="/gameboy.png"
          />
        </div>
        <div class="absolute left-[218px] top-[122px] shadow-inner z-10 w-[327px] aspect-[1.08]">
          <GameboyCanvas />
        </div>
      </div>
    </section>
  );
};

export default Gameboy;
