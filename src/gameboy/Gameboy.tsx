import { invoke } from '@tauri-apps/api/core';
import GameboyCanvas from './GameboyCanvas';
import useDefaultKeymap from '../hooks/useDefaultKeymap';
import { createSignal } from 'solid-js';
import ToggleSwitch from '../components/ToggleSwitch';
import { BiRegularArrowBack } from 'solid-icons/bi';
import { VsSettingsGear } from 'solid-icons/vs';
import { FaSolidPowerOff } from 'solid-icons/fa';

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
    <section class="relative w-[100vw] h-[100vh] flex justify-center items-center overflow-hidden">
      <div class="relative w-full aspect-[1.25] m-auto flex justify-center items-center">
        <img
          class="absolute inset-0 w-full object-cover pointer-events-none"
          src="/gameboy.png"
        />
        <div class="absolute top-[14.15%] w-[51.9%] z-10 aspect-[1.08]">
          <div class="relative w-full h-full">
            <GameboyCanvas enabled={enabled} />
            <div class="absolute inset-0 z-50 shadow-[inset_0px_0px_4px_4px_rgba(0,0,0,0.2)]" />
          </div>
        </div>
      </div>
      <ul class="absolute right-2 bottom-2 menu bg-base-200 rounded-box">
        <li>
          <a on:click={() => onToggle(!enabled())}>
            <FaSolidPowerOff color={enabled() ? 'green' : 'white'} />
          </a>
        </li>
        <li>
          <a
            on:click={async () => {
              await invoke('unload_emulator');
              props.onGoBack();
            }}
          >
            <BiRegularArrowBack />
          </a>
        </li>
        <li>
          <a>
            <VsSettingsGear />
          </a>
        </li>
      </ul>
    </section>
  );
};

export default Gameboy;
