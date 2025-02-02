import './index.css';
import Gameboy from './Gameboy';
import { createSignal, Show } from 'solid-js';

type EmulatorType = 'none' | 'gameboy';

function App() {
  const [emulator, setEmulator] = createSignal<EmulatorType>('none');

  return (
    <main class="container">
      <Show when={emulator() === 'none'}>
        <div>
          <button
            class="py-2 px-4 rounded-sm bg-gray-100 cursor-pointer"
            on:click={() => {
              setEmulator('gameboy');
            }}
          >
            Gameboy
          </button>
        </div>
      </Show>
      <Show when={emulator() === 'gameboy'}>
        <Gameboy />
      </Show>
    </main>
  );
}

export default App;
