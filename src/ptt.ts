import { encodeWav } from "./wav";

const MIN_RECORDING_MS = 400;

export interface Recording {
  wav: Uint8Array;
  durationMs: number;
}

/** Records the microphone while the push-to-talk key is held. */
export class PushToTalk {
  private context?: AudioContext;
  private stream?: MediaStream;
  private processor?: ScriptProcessorNode;
  private chunks: Float32Array[] = [];
  private startedAt = 0;
  private recording = false;

  public async start(): Promise<void> {
    if (this.recording) return;
    this.recording = true;
    this.chunks = [];
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
      this.processor.onaudioprocess = (e) => {
        this.chunks.push(new Float32Array(e.inputBuffer.getChannelData(0)));
      };
      source.connect(this.processor);
      this.processor.connect(this.context.destination);
      this.startedAt = performance.now();
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
