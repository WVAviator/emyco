import { createSignal, createEffect } from 'solid-js';
import { appDataDir, join, BaseDirectory } from '@tauri-apps/api/path';
import { exists, readDir } from '@tauri-apps/plugin-fs';
import RomPicker from './RomPicker';

interface RomListProps {
  onRomSelect: (romName: string) => void;
}

const RomList = ({ onRomSelect }: RomListProps) => {
  const [romDirs, setRomDirs] = createSignal<string[]>([]);

  const fetchRoms = async () => {
    try {
      const appDir = await appDataDir();
      const entries = await readDir(appDir, { baseDir: BaseDirectory.AppData });

      const romFolders: string[] = [];

      for (const entry of entries) {
        if (entry.isDirectory) {
          const romPath = await join(appDir, entry.name, 'rom.gb');
          if (await exists(romPath)) {
            romFolders.push(entry.name);
          }
        }
      }

      setRomDirs(romFolders);
    } catch (error) {
      console.error('Error reading ROM directories:', error);
    }
  };

  createEffect(() => fetchRoms());

  const handleRomClick = (romName: string) => {
    console.log(`Clicked ROM: ${romName}`);
    onRomSelect(romName);
  };

  return (
    <div class="flex flex-col items-center w-full gap-4">
      <h2>Available ROMs</h2>
      <div class="min-h-32 min-w-48 border-2 rounded-md flex flex-col items-center p-2">
        {romDirs().length > 0 ? (
          <ul class="flex flex-col items-center gap-2 p-2">
            {romDirs().map((rom) => (
              <li
                onClick={() => handleRomClick(rom)}
                class="cursor-pointer px-4 py-2"
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
      <RomPicker onRomAdded={fetchRoms} />
    </div>
  );
};

export default RomList;
