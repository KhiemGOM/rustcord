// @components/Switch, as Rustcord's switch.
export const Switch = ({ checked, onChange, disabled }) => VencordCreateElement("fc-switch", { checked: !!checked, onChange, disabled });
