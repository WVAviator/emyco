import { createSignal } from 'solid-js';
import { appDataDir, BaseDirectory, join } from '@tauri-apps/api/path';
import { open } from '@tauri-apps/plugin-dialog';
import { copyFile, mkdir } from '@tauri-apps/plugin-fs';

const RomPicker = () => {
  const [selectedDir, setSelectedDir] = createSignal<string | null>(null);

  const pickFile = async () => {
    try {
      const filePath = await open({
        multiple: false,
        filters: [{ name: 'Gameboy ROMs', extensions: ['gb'] }],
      });

      if (!filePath) return;

      const fileNameWithExt = filePath.split('/').pop();
      if (!fileNameWithExt) return;

      const fileBaseName = fileNameWithExt.replace(/\.gb$/, '');

      const appDir = await appDataDir();
      const newDir = await join(appDir, fileBaseName);

      await mkdir(newDir, { recursive: true });

      const newFilePath = await join(newDir, 'rom.gb');

      await copyFile(filePath as string, newFilePath, {
        toPathBaseDir: BaseDirectory.AppData,
      });

      setSelectedDir(newDir);
    } catch (error) {
      console.error('Error selecting file:', error);
    }
  };

  return (
    <div>
      <button onClick={pickFile}>Select a GameBoy ROM</button>
      {selectedDir() && <p>ROM stored in: {selectedDir()}</p>}
    </div>
  );
};

export default RomPicker;
