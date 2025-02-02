import { invoke } from '@tauri-apps/api/core';
import './App.css';
import GameboyCanvas from './GameboyCanvas';

function App() {
  return (
    <main class="container">
      <div style={{ padding: '8px' }}>
        <GameboyCanvas />
      </div>
      <button
        on:click={() => {
          invoke('start_gameboy');
        }}
      >
        Start
      </button>
    </main>
  );
}

export default App;
