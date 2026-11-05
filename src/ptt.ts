import { encodeWav } from "./wav";

const MIN_RECORDING_MS = 400;
/** A chunk quieter than this (RMS, 0 to 1) counts as silence when listening hands-free. */
const SPEECH_LEVEL = 0.02;

export interface Recording {
  wav: Uint8Array;
  durationMs: number;
}

/** Hands-free listening: stop by itself once the person has spoken and then gone quiet. */
export interface AutoStop {
  /** Quiet for this long after speech ends the recording. */
  silenceMs: number;
  /** Give up if nothing is said within this long. */
  noSpeechMs: number;
  /** Never record longer than this. */
  maxMs: number;
  /** Called once when it is time to stop; the caller then calls `stop()` to collect the audio. */
  onDone: () => void;
}

/** Records the microphone, either while the push-to-talk key is held or hands-free. */
export class PushToTalk {
  private context?: AudioContext;
  private stream?: MediaStream;
  private processor?: ScriptProcessorNode;
  private chunks: Float32Array[] = [];
  private startedAt = 0;
  private recording = false;
  /** Hands-free recordings are only kept when something was actually said. */
  private handsFree = false;
  private heardSpeech = false;

  public isRecording(): boolean {
    return this.recording;
  }

  public async start(auto?: AutoStop): Promise<void> {
    if (this.recording) return;
    this.recording = true;
    this.chunks = [];
    this.handsFree = auto !== undefined;
    this.heardSpeech = false;
    try {
      this.stream = await navigator.mediaDevices.getUserMedia({
        audio: { channelCount: 1, echoCancellation: true, noiseSuppression: true },
      });
      // The key may have been released while the permission prompt was open.
      if (!this.recording) {
        this.release();
        return;
      }
      this.context = new AudioContext({ sampleRate: 16000 });
      const source = this.context.createMediaStreamSource(this.stream);
      this.processor = this.context.createScriptProcessor(4096, 1, 1);
      this.startedAt = performance.now();
      let lastLoud = this.startedAt;
      let notified = false;
      this.processor.onaudioprocess = (e) => {
        const samples = new Float32Array(e.inputBuffer.getChannelData(0));
        this.chunks.push(samples);
        if (!auto || notified) return;
        const now = performance.now();
        let sum = 0;
        for (const s of samples) sum += s * s;
        if (Math.sqrt(sum / samples.length) > SPEECH_LEVEL) {
          this.heardSpeech = true;
          lastLoud = now;
        }
        const total = now - this.startedAt;
        const finished =
          (this.heardSpeech && now - lastLoud > auto.silenceMs) ||
          (!this.heardSpeech && total > auto.noSpeechMs) ||
          total > auto.maxMs;
        if (finished) {
          notified = true;
          auto.onDone();
        }
      };
      source.connect(this.processor);
      this.processor.connect(this.context.destination);
    } catch (e) {
      this.recording = false;
      this.release();
      throw e;
    }
  }

  /** Stops recording. Returns null when the key was tapped (under 0.4 s) or nothing was heard. */
  public stop(): Recording | null {
    if (!this.recording) return null;
    this.recording = false;
    const started = this.startedAt;
    const durationMs = started === 0 ? 0 : performance.now() - started;
    const rate = this.context?.sampleRate ?? 16000;
    const chunks = this.chunks;
    this.release();
    if (this.handsFree && !this.heardSpeech) return null;
    if (durationMs < MIN_RECORDING_MS || chunks.length === 0) return null;
    return { wav: encodeWav(chunks, rate), durationMs };
  }

  private release(): void {
    this.processor?.disconnect();
    this.stream?.getTracks().forEach((t) => t.stop());
    void this.context?.close();
    this.processor = undefined;
    this.stream = undefined;
    this.context = undefined;
    this.startedAt = 0;
  }
}
