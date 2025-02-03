import { invoke } from '@tauri-apps/api/core';
import GameboyCanvas from './GameboyCanvas';
import { onMount } from 'solid-js';

const Gameboy = () => {
  onMount(() => {
    console.log('Setting up Gameboy as current emulator.');
    invoke('setup_gameboy');
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
