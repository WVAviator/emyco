import './index.css';
import Gameboy from './gameboy/Gameboy';
import { createSignal, Show } from 'solid-js';
import RomList from './RomList';
import { invoke } from '@tauri-apps/api/core';

type EmulatorType = 'none' | 'gameboy';

function App() {
  const [emulator, setEmulator] = createSignal<EmulatorType>('none');

  const selectRom = (romName: string) => {
    invoke('setup_gameboy', { name: romName });
    setEmulator('gameboy');
  }

  return (
    <main class="container">
      <Show when={emulator() === 'none'}>
        <div>
          <RomList onRomSelect={selectRom} />
        </div>
      </Show>
      <Show when={emulator() === 'gameboy'}>
        <Gameboy />
      </Show>
    </main>
  );
}

export default App;
