import { createSignal, createEffect } from 'solid-js';
import { appDataDir, BaseDirectory } from '@tauri-apps/api/path';
import { readDir } from '@tauri-apps/plugin-fs';
import RomPicker from './RomPicker';

interface RomListProps {
  onRomSelect: (romName: string) => void;
}

const RomList = ({ onRomSelect }: RomListProps) => {
  const [romFiles, setRomFiles] = createSignal<string[]>([]);

  const fetchRoms = async () => {
    try {
      const appDir = await appDataDir();
      const entries = await readDir(appDir, { baseDir: BaseDirectory.AppData });

      const gbFiles: string[] = entries
        .filter(
          (entry) =>
            !entry.isDirectory && entry.name?.toLowerCase().endsWith('.gb')
        )
        .map((entry) => entry.name.split('.')[0]);

      setRomFiles(gbFiles);
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
        {romFiles().length > 0 ? (
          <ul class="flex flex-col items-center gap-2 p-2">
            {romFiles().map((rom) => (
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
