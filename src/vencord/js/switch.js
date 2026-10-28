// @components/Switch, as Fastcord's switch.
export const Switch = ({ checked, onChange, disabled }) => VencordCreateElement("fc-switch", { checked: !!checked, onChange, disabled });
