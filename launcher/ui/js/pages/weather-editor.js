import { html, useState, useEffect } from '../../vendor/preact-htm.js';
import { t } from '../i18n.js';
import { call } from '../api.js';
import { toast } from '../store.js';
import { Button, Dialog, Select, Slider, Toggle, Spinner } from '../ui.js';
import { choose } from '../data.js';

const PRECIP = [['0', 'None'], ['1', 'Rain'], ['2', 'Snow']];

export const isCustomWeather = (w) => /^(custom|CUSTOM):/.test(w || '');

export const editableWeather = (w) => isCustomWeather(w) || /\.owt$/i.test(w || '');

const percent = (v) => `${Math.round(v * 100)} %`;

const distance = (v) => (v >= 49950 ? t('Unlimited') : v >= 1000 ? `${(v / 1000).toFixed(1)} km` : `${Math.round(v)} m`);

function dewPoint(temp, rh) {
  const g = Math.log(Math.min(100, Math.max(0.1, rh)) / 100) + (17.67 * temp) / (243.5 + temp);
  return (243.5 * g) / (17.67 - g);
}

export function WeatherEditor({ from, onClose }) {
  const [w, setW] = useState(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    call('custom_weather', { weather: from }).then(setW).catch((e) => (toast(e.message, 'error'), onClose()));
  }, [from]);
  const put = (patch) => setW({ ...w, ...patch });
  const use = () => {
    setBusy(true);
    call('custom_weather_text', { custom: w })
      .then((weather) => (choose({ weather }), onClose()))
      .catch((e) => (toast(e.message, 'error'), setBusy(false)));
  };
  const footer = html`
    <${Button} label="Cancel" onClick=${onClose} />
    <${Button} kind="primary" label="Done" disabled=${!w || busy} onClick=${use} />`;
  return html`<${Dialog} class="weather" title="Custom weather" onClose=${onClose} footer=${footer}>
    ${!w ? html`<div class="row" style="justify-content:center"><${Spinner} /></div>` : html`<div class="grid-2">
      <${Slider} label="Visibility" value=${w.visibility_m} min=${50} max=${50000} step=${50} onChange=${(visibility_m) => put({ visibility_m })} format=${distance} />
      <${Slider} label="Brightness" value=${w.brightness} min=${0} max=${1.5} step=${0.05} onChange=${(brightness) => put({ brightness })} format=${percent} />
      <${Slider} label="Wind direction" value=${w.wind_dir} min=${0} max=${355} step=${5} onChange=${(wind_dir) => put({ wind_dir })} format=${(v) => `${Math.round(v)}°`} />
      <${Slider} label="Wind speed" value=${w.wind_speed} min=${0} max=${50} step=${0.5} onChange=${(wind_speed) => put({ wind_speed })} format=${(v) => `${v.toFixed(1)} m/s`} />
      <${Slider} label="Temperature" value=${w.temp_c} min=${-40} max=${50} step=${1} onChange=${(temp_c) => put({ temp_c })} format=${(v) => `${Math.round(v)} °C`} />
      <${Slider} label="Humidity" value=${w.humidity} min=${0} max=${100} step=${1} onChange=${(humidity) => put({ humidity })} format=${(v) => t('{} % | dew point {} °C', [Math.round(v), Math.round(dewPoint(w.temp_c, v))])} />
      <${Slider} label="Cloud cover" value=${w.cloud_cover} min=${0} max=${1} step=${0.01} onChange=${(cloud_cover) => put({ cloud_cover })} format=${(v) => (v <= 0 ? t('Clear sky') : percent(v))} />
      <${Slider} label="Wet roads" value=${w.road_wetness} min=${0} max=${1} step=${0.05} onChange=${(road_wetness) => put({ road_wetness })} format=${percent} />
      <div class="slider">
        <div class="top"><span>${t('Precipitation')}</span><span></span></div>
        <${Select} value=${String(w.precip)} options=${PRECIP} onChange=${(v) => put({ precip: Number(v) })} />
      </div>
      <${Slider} label="Precipitation strength" value=${w.precip_intensity} min=${0} max=${255} step=${1} onChange=${(precip_intensity) => put({ precip_intensity })} format=${(v) => `${Math.round(v)} / 255`} />
      <${Toggle} checked=${w.snow_cover} onChange=${(snow_cover) => put({ snow_cover })} label="Snow cover" />
      <${Toggle} checked=${w.snow_on_road} onChange=${(snow_on_road) => put({ snow_on_road })} label="Snow on road" />
    </div>`}
  </${Dialog}>`;
}
