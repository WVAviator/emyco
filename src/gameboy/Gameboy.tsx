import { invoke } from '@tauri-apps/api/core';
import GameboyCanvas from './GameboyCanvas';
import useDefaultKeymap from '../hooks/useDefaultKeymap';
import { createSignal } from 'solid-js';
import ToggleSwitch from '../components/ToggleSwitch';
import { BiRegularArrowBack } from 'solid-icons/bi';

interface GameboyProps {
  rom: string;
  onGoBack: () => void;
}

const Gameboy = (props: GameboyProps) => {
  const [enabled, setEnabled] = createSignal(false);

  const onToggle = async (checked: boolean) => {
    setEnabled(checked);

    if (checked) {
      console.log('Setting up emulator with ROM {}', props.rom);
      await invoke('setup_gameboy', { name: props.rom });
      await invoke('start_emulator');
    } else {
      console.log('Unloading emulator.');
      await invoke('unload_emulator');
    }
  };

  useDefaultKeymap();

  return (
    <section>
      <div class="flex h-16 gap-4 items-center justify-center w-full">
        <button
          class="btn btn-square btn-outline"
          on:click={async () => {
            await invoke('unload_emulator');
            props.onGoBack();
          }}
        >
          <BiRegularArrowBack />
        </button>
        <ToggleSwitch checked={enabled} setChecked={onToggle}>
          Power
        </ToggleSwitch>
      </div>
      <div class="w-full relative z-0">
        <div class="relative w-full">
          <img
            class="absolute object-cover w-full scale-150 top-28 pointer-events-none"
            src="/gameboy.png"
          />
        </div>
        <div class="absolute left-[218px] top-[122px] shadow-inner z-10 w-[327px] aspect-[1.08]">
          <GameboyCanvas enabled={enabled} />
        </div>
      </div>
    </section>
  );
};

export default Gameboy;
