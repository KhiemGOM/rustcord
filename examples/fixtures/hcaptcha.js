var base = new URL(document.currentScript.src).origin;
function beacon(path, params) {
  (new Image()).src = base + "/hcaptcha/" + path + "?" + new URLSearchParams(params).toString();
}
window.hcaptcha = {
  render: function (id, opts) {
    beacon("render", { sitekey: opts.sitekey, host: location.hostname });
    setTimeout(function () { opts.callback("e2e-captcha"); }, 300);
    return 1;
  },
  setData: function (id, data) {
    beacon("data", { rqdata: data.rqdata });
  },
};
window.ready();
