import { appDataDir, BaseDirectory, join } from '@tauri-apps/api/path';
import { open } from '@tauri-apps/plugin-dialog';
import { copyFile, readFile } from '@tauri-apps/plugin-fs';
import { FiUpload } from 'solid-icons/fi';

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

      const fileData: Uint8Array = await readFile(filePath);

      const start: number = 0x0134;
      const end: number = 0x0144;
      const slice: Uint8Array = fileData.slice(start, end);

      let lastNonZeroIndex: number = slice.length - 1;
      while (lastNonZeroIndex >= 0 && slice[lastNonZeroIndex] === 0) {
        lastNonZeroIndex--;
      }
      const trimmedSlice: Uint8Array = slice.slice(0, lastNonZeroIndex + 1);
      const decoder: TextDecoder = new TextDecoder('ascii');
      const fileInternalName: string = decoder.decode(trimmedSlice).trim();

      const newFileName: string = `${fileInternalName}.gb`;

      const appDir: string = await appDataDir();
      const newFilePath: string = await join(appDir, newFileName);

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
      <button class="btn" onClick={pickFile}>
        <FiUpload />
        Import ROM
      </button>
    </div>
  );
};

export default RomPicker;
