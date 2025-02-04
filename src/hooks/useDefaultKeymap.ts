import { invoke } from "@tauri-apps/api/core";
import { useKeyPress } from "./useKeyPress";


const useDefaultKeymap = () => {
  useKeyPress({
    Escape: {
      keydown: () => {
        console.log('Sending start key down.');
        invoke('register_input', { key: 'start', down: true });
      },
      keyup: () => {
        console.log('Sending start key up.');
        invoke('register_input', { key: 'start', down: false });
      },
    },
    Tab: {
      keydown: () => {
        invoke('register_input', { key: 'select', down: true });
      },
      keyup: () => {
        invoke('register_input', { key: 'select', down: false });
      },
    },
    Enter: {
      keydown: () => {
        invoke('register_input', { key: 'a', down: true });
      },
      keyup: () => {
        invoke('register_input', { key: 'a', down: false });
      },
    },
    Backspace: {
      keydown: () => {
        invoke('register_input', { key: 'b', down: true });
      },
      keyup: () => {
        invoke('register_input', { key: 'b', down: false });
      },
    },
    w: {
      keydown: () => {
        invoke('register_input', { key: 'up', down: true });
      },
      keyup: () => {
        invoke('register_input', { key: 'up', down: false });
      },
    },
    s: {
      keydown: () => {
        invoke('register_input', { key: 'down', down: true });
      },
      keyup: () => {
        invoke('register_input', { key: 'down', down: false });
      },
    },
    a: {
      keydown: () => {
        invoke('register_input', { key: 'left', down: true });
      },
      keyup: () => {
        invoke('register_input', { key: 'left', down: false });
      },
    },
    d: {
      keydown: () => {
        invoke('register_input', { key: 'right', down: true });
      },
      keyup: () => {
        invoke('register_input', { key: 'right', down: false });
      },
    },
  });
}

export default useDefaultKeymap;
