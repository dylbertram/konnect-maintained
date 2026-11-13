"""Author/read synthetic acceptance boards using KiCad's own file model.

This is test infrastructure only. It never opens or changes a user's board.
"""
import json
from pathlib import Path
import sys

import pcbnew


def seed(board_path, specification):
    board = pcbnew.LoadBoard(str(board_path))
    nets = {}
    for name in specification["nets"]:
        net = pcbnew.NETINFO_ITEM(board, name)
        board.Add(net)
        nets[name] = net
    footprints = {}
    for index, component in enumerate(specification["components"]):
        library, name = component["footprint"].split(":", 1)
        footprint = pcbnew.FootprintLoad(
            f"/usr/share/kicad/footprints/{library}.pretty", name
        )
        assert footprint is not None, component["footprint"]
        footprint.SetFPID(pcbnew.LIB_ID(library, name))
        footprint.SetReference(component["reference"])
        footprint.SetValue(component["value"])
        footprint.SetPath(pcbnew.KIID_PATH(component["path"]))
        footprint.SetPosition(pcbnew.VECTOR2I(
            pcbnew.FromMM(30 + index * 15), pcbnew.FromMM(40)
        ))
        footprint.SetOrientationDegrees(90 if index % 2 else 0)
        board.Add(footprint)
        for pad in footprint.Pads():
            net_name = component["pads"].get(pad.GetNumber())
            if net_name is not None:
                pad.SetNet(nets[net_name])
        footprints[component["reference"]] = footprint
    # Route the sheet-local startup-limiter net, not merely a global rail. Its
    # hierarchy-qualified name changes during the transfer.
    first = footprints["R101"].FindPadByNumber("2")
    second = footprints["R102"].FindPadByNumber("1")
    assert first.GetNetname() == second.GetNetname()
    route = pcbnew.PCB_TRACK(board)
    route.SetStart(first.GetPosition())
    route.SetEnd(second.GetPosition())
    route.SetWidth(pcbnew.FromMM(0.25))
    route.SetLayer(pcbnew.F_Cu)
    route.SetNetCode(first.GetNetCode())
    board.Add(route)
    corners = [(10, 10), (130, 10), (130, 90), (10, 90), (10, 10)]
    for start, end in zip(corners, corners[1:]):
        edge = pcbnew.PCB_SHAPE(board)
        edge.SetShape(pcbnew.SHAPE_T_SEGMENT)
        edge.SetStart(pcbnew.VECTOR2I(*(pcbnew.FromMM(v) for v in start)))
        edge.SetEnd(pcbnew.VECTOR2I(*(pcbnew.FromMM(v) for v in end)))
        edge.SetWidth(pcbnew.FromMM(0.05))
        edge.SetLayer(pcbnew.Edge_Cuts)
        board.Add(edge)
    assert pcbnew.SaveBoard(str(board_path), board)


def inspect(board_path):
    board = pcbnew.LoadBoard(str(board_path))
    footprints = {}
    for footprint in board.GetFootprints():
        footprints[footprint.GetReference()] = {
            "uuid": footprint.m_Uuid.AsString(),
            "path": footprint.GetPath().AsString(),
            "position": [footprint.GetPosition().x, footprint.GetPosition().y],
            "rotation": footprint.GetOrientationDegrees(),
            "layer": footprint.GetLayer(),
            "pads": {
                pad.GetNumber(): {
                    "uuid": pad.m_Uuid.AsString(), "net": pad.GetNetname(),
                    "position": [pad.GetPosition().x, pad.GetPosition().y],
                    "size": [pad.GetSize().x, pad.GetSize().y],
                    "rotation": pad.GetOrientation().AsDegrees(),
                }
                for pad in footprint.Pads()
            },
        }
    tracks = {
        track.m_Uuid.AsString(): {
            "start": [track.GetStart().x, track.GetStart().y],
            "end": [track.GetEnd().x, track.GetEnd().y],
            "width": track.GetWidth(),
            "layer": track.GetLayer(),
            "net": track.GetNetname(),
        }
        for track in board.GetTracks()
    }
    return {"footprints": footprints, "tracks": tracks}


if __name__ == "__main__":
    mode, path = sys.argv[1:3]
    if mode == "seed":
        seed(Path(path), json.loads(Path(sys.argv[3]).read_text()))
    elif mode != "inspect":
        raise SystemExit(f"Unknown mode {mode}")
    print(json.dumps(inspect(Path(path))))
