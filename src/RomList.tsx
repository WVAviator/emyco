import { createSignal, createEffect, For } from 'solid-js';
import { appDataDir, BaseDirectory, join } from '@tauri-apps/api/path';
import { readDir, readFile } from '@tauri-apps/plugin-fs';
import RomPicker from './RomPicker';
import { extractRomMetadata, GameboyRomMetadata } from './utilities/metadata';

interface RomListProps {
  onRomSelect: (romName: string) => void;
}

const RomList = ({ onRomSelect }: RomListProps) => {
  const [roms, setRoms] = createSignal<GameboyRomMetadata[]>([]);

  const fetchRoms = async () => {
    try {
      const appDir = await appDataDir();
      const entries = await readDir(appDir, { baseDir: BaseDirectory.AppData });

      const romData: GameboyRomMetadata[] = await Promise.all(
        entries
          .filter(
            (entry) =>
              !entry.isDirectory && entry.name?.toLowerCase().endsWith('.gb')
          )
          .map(async (entry) => {
            const filePath = await join(appDir, entry.name);
            const romData = await readFile(filePath);
            return extractRomMetadata(romData);
          })
      );

      setRoms(romData);
    } catch (error) {
      console.error('Error reading ROM directories:', error);
    }
  };

  createEffect(() => fetchRoms());

  const handleRomClick = (rom: GameboyRomMetadata) => {
    console.log(`Clicked ROM: ${rom.title}`);
    onRomSelect(rom.title);
  };

  return (
    <div class="flex flex-col items-center w-full gap-4">
      <h2>Available ROMs</h2>
      <div class="overflow-x-auto">
        <table class="table-md">
          <thead>
            <tr class="text-left">
              <th>Game Title</th>
              <th>Publisher</th>
              <th>ROM Size</th>
              <th>RAM Size</th>
            </tr>
          </thead>
          <tbody>
            <For
              each={roms()}
              fallback={
                <tr>
                  <td>No ROMs Available</td>
                </tr>
              }
            >
              {(rom: GameboyRomMetadata) => (
                <tr
                  class="hover:bg-base-200 cursor-pointer"
                  on:click={() => handleRomClick(rom)}
                >
                  <td class="whitespace-nowrap">{rom.formattedTitle}</td>
                  <td>{rom.licensee}</td>
                  <td>{rom.romSize}</td>
                  <td>{rom.ramSize}</td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </div>
      <RomPicker onRomAdded={fetchRoms} />
    </div>
  );
};

export default RomList;
