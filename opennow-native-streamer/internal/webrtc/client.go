package webrtc

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"strconv"
	"strings"
	"sync"

	"github.com/OpenCloudGaming/OpenNOW/opennow-native-streamer/internal/input"
	"github.com/OpenCloudGaming/OpenNOW/opennow-native-streamer/internal/media"
	"github.com/OpenCloudGaming/OpenNOW/opennow-native-streamer/pkg/protocol"
	"github.com/pion/rtcp"
	pion "github.com/pion/webrtc/v4"
)

type EventSink interface {
	Emit(string, any) error
}

type Client struct {
	mu                sync.Mutex
	pc                *pion.PeerConnection
	media             media.Player
	events            EventSink
	encoder           input.Encoder
	settings          protocol.LaunchSettings
	session           protocol.SessionInfo
	window            protocol.WindowSettings
	controlChannel    *pion.DataChannel
	reliableChannel   *pion.DataChannel
	mouseChannel      *pion.DataChannel
	partialReliableMs int
	mediaStartedOnce  sync.Once
}

func New(events EventSink) *Client {
	return &Client{events: events}
}

func (c *Client) StartSession(ctx context.Context, req protocol.StartSessionRequest) error {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.session = req.Session
	c.settings = req.Settings
	c.window = req.Window
	if c.window.Title == "" {
		c.window.Title = protocol.WindowTitle
	}
	if c.window.Width == 0 {
		c.window.Width = 1280
	}
	if c.window.Height == 0 {
		c.window.Height = 720
	}
	c.media = media.New()
	return c.media.Start(ctx, media.Config{
		WindowTitle: c.window.Title,
		Width:       c.window.Width,
		Height:      c.window.Height,
		Codec:       c.settings.Codec,
		InputSink:   c.handleWindowInput,
	})
}

func (c *Client) HandleOffer(offer protocol.SignalOffer) error {
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.pc != nil {
		_ = c.pc.Close()
	}
	c.mediaStartedOnce = sync.Once{}
	config := pion.Configuration{ICEServers: toICEServers(c.session.IceServers)}
	pc, err := pion.NewPeerConnection(config)
	if err != nil {
		return err
	}
	c.pc = pc
	c.partialReliableMs = ParsePartialReliableThresholdMs(offer.SDP)

	pc.OnICECandidate(func(candidate *pion.ICECandidate) {
		if candidate == nil {
			return
		}
		init := candidate.ToJSON()
		index := init.SDPMLineIndex
		payload := protocol.IceCandidate{Candidate: init.Candidate, SDPMid: valueOrEmpty(init.SDPMid), SDPMLineIndex: index, UsernameFragment: valueOrEmpty(init.UsernameFragment)}
		_ = c.events.Emit("local-ice", payload)
	})
	pc.OnConnectionStateChange(func(state pion.PeerConnectionState) {
		_ = c.events.Emit("state", protocol.NativeState{Status: string(state), Message: "peer-connection"})
	})
	pc.OnTrack(func(track *pion.TrackRemote, receiver *pion.RTPReceiver) {
		go c.consumeTrack(track, receiver)
	})
	pc.OnDataChannel(func(dc *pion.DataChannel) {
		if dc.Label() == "control_channel" {
			c.controlChannel = dc
			dc.OnMessage(func(msg pion.DataChannelMessage) {
				var raw any
				_ = json.Unmarshal(msg.Data, &raw)
			})
		}
	})

	orderedTrue := true
	orderedFalse := false
	if c.reliableChannel, err = pc.CreateDataChannel("input_channel_v1", &pion.DataChannelInit{Ordered: &orderedTrue}); err != nil {
		return err
	}
	if c.mouseChannel, err = pc.CreateDataChannel("input_channel_partially_reliable", &pion.DataChannelInit{Ordered: &orderedFalse, MaxPacketLifeTime: uint16Ptr(uint16(c.partialReliableMs))}); err != nil {
		return err
	}

	processedOffer := FixServerIP(offer.SDP, coalesce(mediaConnectionInfoIP(c.session), c.session.ServerIP))
	processedOffer = PreferCodec(processedOffer, c.settings.Codec)
	if err = pc.SetRemoteDescription(pion.SessionDescription{Type: pion.SDPTypeOffer, SDP: processedOffer}); err != nil {
		return err
	}
	answer, err := pc.CreateAnswer(nil)
	if err != nil {
		return err
	}
	answer.SDP = MungeAnswerSDP(answer.SDP, int(c.settings.MaxBitrateMbps*1000))
	if err = pc.SetLocalDescription(answer); err != nil {
		return err
	}
	<-pion.GatheringCompletePromise(pc)
	finalSDP := pc.LocalDescription().SDP
	creds := ExtractIceCredentials(finalSDP)
	nvst := BuildNvstSDP(NvstParams{
		Width:                    resolutionWidth(c.settings.Resolution),
		Height:                   resolutionHeight(c.settings.Resolution),
		ClientViewportWidth:      c.window.Width,
		ClientViewportHeight:     c.window.Height,
		FPS:                      c.settings.FPS,
		MaxBitrateKbps:           int(c.settings.MaxBitrateMbps * 1000),
		PartialReliableThreshold: c.partialReliableMs,
		Codec:                    c.settings.Codec,
		ColorQuality:             c.settings.ColorQuality,
		Credentials:              creds,
	})
	if err = c.events.Emit("local-answer", protocol.LocalAnswer{SDP: finalSDP, NvstSDP: nvst}); err != nil {
		return err
	}
	if err = c.injectManualCandidate(pc, ExtractIceUfragFromOffer(processedOffer)); err != nil {
		_ = c.events.Emit("error", protocol.NativeError{Code: "manual-ice", Message: err.Error(), Fatal: false})
	}
	return c.events.Emit("state", protocol.NativeState{Status: "connecting", Message: "offer handled"})
}

func (c *Client) AddRemoteICE(candidate protocol.IceCandidate) error {
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.pc == nil {
		return nil
	}
	init := pion.ICECandidateInit{Candidate: candidate.Candidate}
	if candidate.SDPMid != "" {
		init.SDPMid = &candidate.SDPMid
	}
	if candidate.SDPMLineIndex != nil {
		init.SDPMLineIndex = candidate.SDPMLineIndex
	}
	if candidate.UsernameFragment != "" {
		init.UsernameFragment = &candidate.UsernameFragment
	}
	return c.pc.AddICECandidate(init)
}

func (c *Client) HandleInput(message protocol.InputMessage) error {
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.reliableChannel == nil || c.mouseChannel == nil {
		return nil
	}
	switch message.Kind {
	case "keyboard":
		var payload protocol.KeyboardInput
		if err := json.Unmarshal(message.Payload, &payload); err != nil {
			return err
		}
		return c.reliableChannel.Send(c.encoder.EncodeKeyboard(input.KeyboardPayload{Keycode: payload.Keycode, Scancode: payload.Scancode, Modifiers: payload.Modifiers, Down: payload.Down}))
	case "mouse-move":
		var payload protocol.MouseMoveInput
		if err := json.Unmarshal(message.Payload, &payload); err != nil {
			return err
		}
		return c.mouseChannel.Send(c.encoder.EncodeMouseMove(input.MouseMovePayload{DX: payload.DX, DY: payload.DY}))
	case "mouse-button":
		var payload protocol.MouseButtonInput
		if err := json.Unmarshal(message.Payload, &payload); err != nil {
			return err
		}
		return c.reliableChannel.Send(c.encoder.EncodeMouseButton(input.MouseButtonPayload{Button: payload.Button, Down: payload.Down}))
	case "mouse-wheel":
		var payload protocol.MouseWheelInput
		if err := json.Unmarshal(message.Payload, &payload); err != nil {
			return err
		}
		return c.reliableChannel.Send(c.encoder.EncodeMouseWheel(input.MouseWheelPayload{Delta: payload.Delta}))
	case "gamepad":
		var payload protocol.GamepadInput
		if err := json.Unmarshal(message.Payload, &payload); err != nil {
			return err
		}
		return c.reliableChannel.Send(c.encoder.EncodeGamepad(input.GamepadPayload(payload)))
	default:
		return nil
	}
}

func (c *Client) handleWindowInput(message protocol.InputMessage) error {
	return c.HandleInput(message)
}

func (c *Client) RequestKeyframe() error {
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.pc == nil {
		return nil
	}
	for _, receiver := range c.pc.GetReceivers() {
		track := receiver.Track()
		if track != nil && track.Kind() == pion.RTPCodecTypeVideo {
			_ = c.pc.WriteRTCP([]rtcp.Packet{&rtcp.PictureLossIndication{MediaSSRC: uint32(track.SSRC())}})
		}
	}
	return nil
}

func (c *Client) Close() error {
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.pc != nil {
		_ = c.pc.Close()
		c.pc = nil
	}
	if c.media != nil {
		_ = c.media.Close()
		c.media = nil
	}
	return nil
}

func (c *Client) consumeTrack(track *pion.TrackRemote, _ *pion.RTPReceiver) {
	codec := strings.ToUpper(track.Codec().MimeType)
	for {
		packet, _, err := track.ReadRTP()
		if err != nil {
			if err != io.EOF {
				_ = c.events.Emit("error", protocol.NativeError{Code: "track-read", Message: err.Error(), Fatal: false})
			}
			return
		}
		raw, err := packet.Marshal()
		if err != nil {
			continue
		}
		c.mediaStartedOnce.Do(func() {
			_ = c.events.Emit("state", protocol.NativeState{
				Status:  "media-ready",
				Message: fmt.Sprintf("first %s RTP packet received", codec),
			})
		})
		c.mu.Lock()
		currentMedia := c.media
		if currentMedia == nil {
			c.mu.Unlock()
			return
		}
		if strings.Contains(codec, "VIDEO") {
			_ = currentMedia.PushVideoRTP(raw)
		} else {
			_ = currentMedia.PushAudioRTP(raw)
		}
		c.mu.Unlock()
	}
}

func (c *Client) injectManualCandidate(pc *pion.PeerConnection, serverUfrag string) error {
	if c.session.MediaConnectionInfo == nil {
		return nil
	}
	ip := ExtractPublicIP(c.session.MediaConnectionInfo.IP)
	if ip == "" || c.session.MediaConnectionInfo.Port <= 0 {
		return fmt.Errorf("invalid mediaConnectionInfo")
	}
	candidates := []string{
		fmt.Sprintf("candidate:1 1 udp 2130706431 %s %d typ host", ip, c.session.MediaConnectionInfo.Port),
		fmt.Sprintf("candidate:1 1 tcp 1518149375 %s %d typ host tcptype active", ip, c.session.MediaConnectionInfo.Port),
	}
	for _, cand := range candidates {
		for _, mid := range []string{"0", "1", "2", "3"} {
			idx := parseMid(mid)
			init := pion.ICECandidateInit{Candidate: cand, SDPMid: &mid, SDPMLineIndex: &idx}
			if serverUfrag != "" {
				init.UsernameFragment = &serverUfrag
			}
			if err := pc.AddICECandidate(init); err == nil {
				return nil
			}
		}
	}
	return fmt.Errorf("manual ICE injection failed")
}

func toICEServers(servers []protocol.IceServer) []pion.ICEServer {
	out := make([]pion.ICEServer, 0, len(servers))
	for _, server := range servers {
		out = append(out, pion.ICEServer{URLs: server.URLs, Username: server.Username, Credential: server.Credential})
	}
	return out
}

func mediaConnectionInfoIP(s protocol.SessionInfo) string {
	if s.MediaConnectionInfo == nil {
		return ""
	}
	return s.MediaConnectionInfo.IP
}

func valueOrEmpty(v *string) string {
	if v == nil {
		return ""
	}
	return *v
}
func uint16Ptr(v uint16) *uint16 { return &v }
func coalesce(v, fallback string) string {
	if v != "" {
		return v
	}
	return fallback
}
func parseMid(v string) uint16 {
	parsed, _ := strconv.Atoi(v)
	return uint16(parsed)
}
func resolutionWidth(v string) int {
	parts := strings.Split(v, "x")
	if len(parts) != 2 {
		return 1920
	}
	n, _ := strconv.Atoi(parts[0])
	if n == 0 {
		return 1920
	}
	return n
}
func resolutionHeight(v string) int {
	parts := strings.Split(v, "x")
	if len(parts) != 2 {
		return 1080
	}
	n, _ := strconv.Atoi(parts[1])
	if n == 0 {
		return 1080
	}
	return n
}
