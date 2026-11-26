#!/usr/bin/env python3
"""Reference action runner for `vts-extract extract --actions`.

vts-extract cuts the clips, sizes them and writes the prompts; this
program only runs one VideoAgentTrek inverse-dynamics model over them.
It speaks protocol 1 (see crates/extractor/src/actions/backend.rs): one
JSON batch on stdin, `{"responses": [{"id", "text"}]}` on stdout, and
progress on stderr. vts-extract starts it once per stage, so each model
loads once.

The inference calls mirror the reference pipeline
(xlang-ai/VideoAgentTrek, video2action/stages/keyframe_detector.py and
action_identifier.py): Qwen2.5-VL under Hugging Face transformers, the
clip passed as a video with the frame rate and size vts-extract chose.

Requirements (yours to install; vts-extract never installs Python
packages): Python 3.10+, torch, transformers with Qwen2.5-VL support
(4.49 or later), qwen-vl-utils, and a GPU that holds a 7B bf16 model
(about 17 GB of memory). Not exercised by the project's CI, which has
neither the GPU nor the 33 GB of weights; the protocol side is tested
with a scripted stand-in.

Usage: vts-extract extract <video> --actions --actions-runner scripts/idm-runner.py
(on Windows, point --actions-runner at a .cmd that runs `python idm-runner.py`).
"""

import json
import sys

PROTOCOL = 1


def log(message):
    print(f"idm-runner: {message}", file=sys.stderr, flush=True)


def messages_for(request):
    return [
        {"role": "system", "content": [{"type": "text", "text": request["system"]}]},
        {
            "role": "user",
            "content": [
                {
                    "type": "video",
                    "video": "file://" + request["clip"],
                    "fps": request["fps"],
                    "resized_height": request["resized_height"],
                    "resized_width": request["resized_width"],
                },
                {"type": "text", "text": request["prompt"]},
            ],
        },
    ]


def main():
    batch = json.load(sys.stdin)
    if batch.get("protocol") != PROTOCOL:
        log(f"unsupported protocol {batch.get('protocol')!r}, expected {PROTOCOL}")
        return 2

    import torch  # noqa: F401  (transformers needs it; fail early if absent)
    from qwen_vl_utils import process_vision_info
    from transformers import AutoProcessor, Qwen2_5_VLForConditionalGeneration

    model_dir = batch["model_dir"]
    log(f"loading {model_dir} for {batch['stage']} ({len(batch['requests'])} clips)")
    processor = AutoProcessor.from_pretrained(model_dir)
    processor.tokenizer.padding_side = "left"
    model = Qwen2_5_VLForConditionalGeneration.from_pretrained(
        model_dir, torch_dtype="auto", device_map="auto"
    )

    responses = []
    for index, request in enumerate(batch["requests"], 1):
        try:
            messages = messages_for(request)
            text = processor.apply_chat_template(
                messages, tokenize=False, add_generation_prompt=True
            )
            images, videos, video_kwargs = process_vision_info(
                messages, return_video_kwargs=True
            )
            inputs = processor(
                text=[text],
                images=images,
                videos=videos,
                padding=True,
                return_tensors="pt",
                **video_kwargs,
            ).to(model.device)
            generated = model.generate(**inputs, max_new_tokens=request["max_new_tokens"])
            answer_ids = generated[:, inputs["input_ids"].shape[1]:]
            answer = processor.batch_decode(answer_ids, skip_special_tokens=True)[0]
            responses.append({"id": request["id"], "text": answer})
            log(f"[{index}/{len(batch['requests'])}] {request['id']}")
        except Exception as error:  # one bad clip must not lose the batch
            log(f"{request['id']}: {error}")

    json.dump({"responses": responses}, sys.stdout)
    return 0


if __name__ == "__main__":
    sys.exit(main())
