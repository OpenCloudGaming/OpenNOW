export type ComposedRecordingResources = {
  composed: MediaStream;
  audioContext: AudioContext;
  dispose: () => void;
};

/**
 * Build the same video + mixed game/mic stream used for manual recording and instant replay.
 */
export function buildComposedRecordingStream(input: {
  videoElement: HTMLVideoElement;
  gameAudioElement: HTMLAudioElement | null;
  micTrack: MediaStreamTrack | null;
}): ComposedRecordingResources | null {
  const stream = input.videoElement.srcObject;
  if (!(stream instanceof MediaStream)) {
    return null;
  }

  const audioCtx = new AudioContext();
  const audioDest = audioCtx.createMediaStreamDestination();

  const gameAudioStream = input.gameAudioElement?.srcObject instanceof MediaStream ? input.gameAudioElement.srcObject : null;
  if (gameAudioStream && gameAudioStream.getAudioTracks().length > 0) {
    audioCtx.createMediaStreamSource(gameAudioStream).connect(audioDest);
  }

  if (input.micTrack && input.micTrack.readyState === "live") {
    const micStream = new MediaStream([input.micTrack]);
    audioCtx.createMediaStreamSource(micStream).connect(audioDest);
  }

  const composed = new MediaStream([...stream.getVideoTracks(), ...audioDest.stream.getAudioTracks()]);

  return {
    composed,
    audioContext: audioCtx,
    dispose: () => {
      void audioCtx.close();
    },
  };
}
