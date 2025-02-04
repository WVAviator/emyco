import { Accessor, JSXElement } from 'solid-js';


interface SwitchProps {
  checked: Accessor<boolean>;
  setChecked: (checked: boolean) => void;
children: JSXElement;
  
}

const ToggleSwitch = (props: SwitchProps) => {
  return (
    <div class="form-control">
      <label class="label cursor-pointer">
        <span class="label-text">{props.children}</span>
        <input type="checkbox" class="toggle" checked={props.checked()} onChange={(e) => props.setChecked(e.currentTarget.checked)}/>
      </label>
    </div>
  )
}

export default ToggleSwitch;
