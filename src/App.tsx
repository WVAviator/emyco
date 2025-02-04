import './index.css';
import Gameboy from './gameboy/Gameboy';
import { createSignal, Show } from 'solid-js';
import RomList from './RomList';

type EmulatorType = 'none' | 'gameboy';

function App() {
  const [emulator, setEmulator] = createSignal<EmulatorType>('none');
  const [rom, setRom] = createSignal<string | null>(null);

  const selectRom = (romName: string) => {
    setEmulator('gameboy');
    setRom(romName);
  };

  return (
    <main class="container">
      <Show when={emulator() === 'none'}>
        <div>
          <RomList onRomSelect={selectRom} />
        </div>
      </Show>
      <Show when={emulator() === 'gameboy' && rom != null}>
        <Gameboy
          rom={rom()!}
          onGoBack={() => {
            setRom(null);
            setEmulator('none');
          }}
        />
      </Show>
    </main>
  );
}

export default App;
