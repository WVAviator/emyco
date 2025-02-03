import { onCleanup } from 'solid-js';

type KeyHandler = (event: KeyboardEvent) => void;

export const useKeyPress = (
  keyMap: Record<string, { keydown?: KeyHandler; keyup?: KeyHandler }>
) => {
  const pressedKeys = new Set<string>();

  const handleKeydown = (event: KeyboardEvent) => {
    if (!pressedKeys.has(event.key)) {
      pressedKeys.add(event.key);
      keyMap[event.key]?.keydown?.(event);
    }
  };

  const handleKeyup = (event: KeyboardEvent) => {
    pressedKeys.delete(event.key);
    const handler = keyMap[event.key]?.keyup;
    if (handler) handler(event);
  };

  window.addEventListener('keydown', handleKeydown);
  window.addEventListener('keyup', handleKeyup);

  onCleanup(() => {
    window.removeEventListener('keydown', handleKeydown);
    window.removeEventListener('keyup', handleKeyup);
  });
};
