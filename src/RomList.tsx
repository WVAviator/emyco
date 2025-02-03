import { createSignal, createEffect } from 'solid-js';
import { appDataDir, join, BaseDirectory } from '@tauri-apps/api/path';
import { exists, readDir } from '@tauri-apps/plugin-fs';

const RomList = () => {
  const [romDirs, setRomDirs] = createSignal<string[]>([]);

  createEffect(async () => {
    try {
      const appDir = await appDataDir();
      const entries = await readDir(appDir, { baseDir: BaseDirectory.AppData });

      const romFolders: string[] = [];

      for (const entry of entries) {
        if (entry.isDirectory) {
          const romPath = await join(entry.name, 'rom.gb');
          if (await exists(romPath)) {
            romFolders.push(entry.name);
          }
        }
      }

      setRomDirs(romFolders);
    } catch (error) {
      console.error('Error reading ROM directories:', error);
    }
  });

  const handleRomClick = (romName: string) => {
    console.log(`Clicked ROM: ${romName}`);
    // TODO: Do something with the selected ROM
  };

  return (
    <div>
      <h2>Available ROMs</h2>
      {romDirs().length > 0 ? (
        <ul>
          {romDirs().map((rom) => (
            <li
              onClick={() => handleRomClick(rom)}
              style={{ cursor: 'pointer', margin: '5px 0' }}
            >
              {rom}
            </li>
          ))}
        </ul>
      ) : (
        <p>No ROMs found.</p>
      )}
    </div>
  );
};

export default RomList;
