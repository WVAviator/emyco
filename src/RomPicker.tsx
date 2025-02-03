import { appDataDir, BaseDirectory, join } from '@tauri-apps/api/path';
import { open } from '@tauri-apps/plugin-dialog';
import { copyFile, mkdir } from '@tauri-apps/plugin-fs';

interface RomPickerProps {
  onRomAdded: () => void;
}

const RomPicker = ({ onRomAdded }: RomPickerProps) => {
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

      onRomAdded();
    } catch (error) {
      console.error('Error selecting file:', error);
    }
  };

  return (
    <div>
      <button
        class="cursor-pointer bg-gray-200 rounded-md px-4 py-2"
        onClick={pickFile}
      >
        Import ROM
      </button>
    </div>
  );
};

export default RomPicker;
